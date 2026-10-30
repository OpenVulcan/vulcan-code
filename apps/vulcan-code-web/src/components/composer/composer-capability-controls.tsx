import React from "react";
import { SessionLifetimeUsagePanel } from "@/components/conversation/session-lifetime-usage";
import { Button } from "@/ui-kit/primitives/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/ui-kit/primitives/popover";
import { InlineStreamPlaceholder, StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import {
  AlertTriangle,
  LoaderCircle,
  LockKeyhole,
  RotateCcw,
  RotateCw,
  SlidersHorizontal,
  ChevronDown,
} from "lucide-react";

import {
  SegmentedControl,
  SegmentedControlItem,
} from "@/ui-kit/primitives/choice-controls";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/ui-kit/primitives/dialog";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog";
import { HOVER_CARD_CLOSE_DELAY_MS, HOVER_CARD_OPEN_DELAY_MS, HoverCard, HoverCardContent, HoverCardTrigger } from "@/ui-kit/primitives/hover-card";
import { groupProviderToolCapabilities } from "@/components/shared/provider-tool-capability-groups";
import {
  ComposerCapabilitySummary,
} from "@/components/composer/composer-capability-summary";
import { ProviderToolCapabilityIcon } from "@/components/shared/provider-tool-capability-icon";
import type {
  ComposerProviderToolSettingsSnapshot,
} from "@/components/composer/composer-model";
import {
  isMediaUnderstandingCapability,
  providerToolCapabilityColor,
  providerToolCapabilityLabel,
} from "@/components/composer/composer-capability-model";
import {
  useComposerCapabilitySettings,
} from "@/components/composer/use-composer-capability-settings";
import {
  ComposerCapabilityDirectRoute,
  ComposerCapabilityExtensionList,
  ComposerCapabilityKindList,
  ComposerCapabilityMediaRoute,
} from "@/components/composer/composer-capability-dialog-sections";
import { useI18n, type TranslationKey } from "@/i18n";
import { formatTokenCount, tokenUnitLabel } from "@/lib/usage-format";
import {
  isDirectProviderToolCapability,
  isModelExtensionCapability,
} from "@/lib/provider-tool-capabilities";
import type {
  ContextUsageBreakdownItem,
  ContextCompactionWindowNodeId,
  ContextUsageSnapshot,
  PublicAiContextCompactionWindowNode,
  PublicAiProviderToolCapabilityKind,
  SessionProviderUsageSummary,
} from "@/types";
import { errorMessage } from "@/lib/display-format";
import { useDedicatedComposer } from "@/components/composer/dedicated/dedicated-session-controls";

// Settings model helpers remain importable from this module for existing callers and tests.
// 设置模型辅助函数仍可从本模块导入，供现有调用方与测试使用。
export {
  composerCapabilityIsEnabled,
  isMediaUnderstandingCapability,
  orderedProviderToolCapabilities,
  orderProviderToolCapabilities,
  providerToolCapabilitySwitchDisabled,
  providerToolSettingsConflicts,
  supportedMediaReplacementReasoningEffort,
} from "@/components/composer/composer-capability-model";

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
  // hoverTimer holds the pending hover open or hover close; pointer moves and unmounting cancel it.
  // hoverTimer 保存待执行的悬停打开或悬停关闭；指针移动与组件卸载会取消它。
  const hoverTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  // openedByHover marks a peek: it must not steal composer focus and it closes once the pointer leaves the
  // trigger and the card. Pressing inside the card turns the peek into a pinned popover.
  // openedByHover 标记悬停预览：不得抢走输入区焦点，且指针离开触发器与卡片后关闭。在卡片内按下会将预览固定。
  const openedByHover = React.useRef(false);
  // Cancels a scheduled hover open or close; no input or return value is needed.
  // 取消已安排的悬停打开或关闭；无需输入或返回值。
  const cancelHoverOpen = React.useCallback(() => {
    if (hoverTimer.current !== null) clearTimeout(hoverTimer.current);
    hoverTimer.current = null;
  }, []);
  // Schedules closing a hover peek after the pointer has left both the trigger and the card.
  // 指针离开触发器与卡片后，安排关闭悬停预览。
  const scheduleHoverClose = React.useCallback(() => {
    cancelHoverOpen();
    if (!openedByHover.current) return;
    hoverTimer.current = setTimeout(() => {
      hoverTimer.current = null;
      if (openedByHover.current) setStatisticsOpen(false);
    }, HOVER_CARD_CLOSE_DELAY_MS);
  }, [cancelHoverOpen]);
  React.useEffect(() => cancelHoverOpen, [cancelHoverOpen]);
  if (!props.snapshot) {
    return null;
  }
  const snapshot = props.snapshot;
  const breakdown = contextUsageBreakdown(snapshot);
  const usageBasisPoints = contextUsageBasisPoints(snapshot);
  const percent = usageBasisPoints / 100;
  const title = t("contextUsage.title");
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
  // Persists a validated switch value, including explicit reselection to repair a stale service mirror.
  // 持久化已校验的开关值，包括用于修复陈旧服务镜像的显式重选。
  // value is the node id or default sentinel; returns immediately while one acknowledged mutation runs.
  // value 是节点标识或默认哨兵；在一个等待确认的修改执行期间立即返回。
  const handleNodeSelection = (value: string): void => {
    // nextNodeId comes only from the current backend-owned node catalog.
    // nextNodeId 仅来自当前后端持有的节点目录。
    const nextNodeId = contextWindowNodeIdFromSwitchValue(value, props.contextCompactionWindowNodes);
    if (nextNodeId === undefined || nodeMutation.current) return;
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
  };
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
  const style = { "--vk-ctx-progress": `${percent * 3.6}deg` } as React.CSSProperties;

  return (
    <Popover open={statisticsOpen} onOpenChange={(open) => { cancelHoverOpen(); setStatisticsOpen(open); }}>
      <PopoverTrigger asChild>
        <button
          aria-label={contextUsageAriaLabel}
          className="vk-ctx-button"
          data-locked={selectedNode ? "true" : undefined}
          type="button"
          onPointerEnter={(event) => {
            if (event.pointerType !== "mouse") return;
            cancelHoverOpen();
            if (statisticsOpen) return;
            hoverTimer.current = setTimeout(() => {
              hoverTimer.current = null;
              openedByHover.current = true;
              setStatisticsOpen(true);
            }, HOVER_CARD_OPEN_DELAY_MS);
          }}
          onPointerLeave={(event) => {
            if (event.pointerType === "mouse" && statisticsOpen) scheduleHoverClose();
            else cancelHoverOpen();
          }}
          onPointerDown={() => { cancelHoverOpen(); openedByHover.current = false; }}
        >
          <span
            aria-hidden="true"
            className="vk-ctx-ring"
            data-severity={snapshot.severity}
            data-threshold={thresholdState}
            style={style}
          />
          {selectedNode ? (
            <span className="vk-ctx-lock">
              <LockKeyhole aria-hidden="true" size={11} strokeWidth={2} />
              {formatContextWindowNodeTokens(selectedNode.tokens)}
            </span>
          ) : null}
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        aria-label={title}
        className="vk-stats"
        data-width="wide"
        collisionPadding={12}
        ref={contentRef}
        onOpenAutoFocus={(event) => { event.preventDefault(); if (!openedByHover.current) contentRef.current?.focus(); }}
        onCloseAutoFocus={(event) => { if (openedByHover.current) event.preventDefault(); openedByHover.current = false; }}
        onPointerEnter={cancelHoverOpen}
        onPointerLeave={(event) => { if (event.pointerType === "mouse") scheduleHoverClose(); }}
        onPointerDown={() => { cancelHoverOpen(); openedByHover.current = false; }}
        side="top"
        sideOffset={8}
      >
        <div className="vk-sum-title">
          <strong>{title}</strong>
          <span>{formatContextUsageTotal(snapshot, locale)}</span>
        </div>
        <ContextUsageProgressBar breakdown={breakdown} usageBasisPoints={usageBasisPoints} />
        {detailVisible || snapshot.cacheHitBasisPoints != null ? (
          <div className="vk-ctx-legend">
            {breakdown.map((item) => (
              <div className="vk-stat" data-ctx-kind={item.kind} key={item.kind}>
                <span>
                  <i className="vk-ctx-dot" />
                  {contextUsageKindLabel(item, t)}
                </span>
                <span>{formatBasisPoints(item.basisPoints)}</span>
              </div>
            ))}
            {snapshot.cacheHitBasisPoints != null ? <div className="vk-stat" data-ctx-cache>
              <span>
                <RotateCw className="vk-ctx-dot" aria-hidden="true" size={10} />
                {t("composer.contextUsage.cacheHitRate")}
              </span>
              <strong>{formatBasisPoints(snapshot.cacheHitBasisPoints)}</strong>
            </div> : null}
          </div>
        ) : null}
        <SessionLifetimeUsagePanel usage={providerUsage?.lifetime} error={providerUsage?.lifetimeError} />
        <div className="vk-ctx-window">
          <div className="vk-sum-title">
            <strong>{t("composer.contextWindow.title")}</strong>
            <span>{selectedNode ? t("composer.capabilities.lockedForThisSession") : t("composer.capabilities.followsTheModel")}</span>
          </div>
          <SegmentedControl
            aria-label={t("composer.contextWindow.title")}
            disabled={nodeSaving}
            onValueChange={handleNodeSelection}
            value={props.selectedNodeId ?? "default"}
          >
            {availableNodes.map((node) => (
              <SegmentedControlItem key={node.id} value={node.id}
                // Shared radio state intentionally suppresses unchanged values; only this control resubmits explicit clicks.
                // 共享单选状态有意忽略同值；仅此控件重新提交显式点击。
                onClick={() => { if (props.selectedNodeId === node.id) handleNodeSelection(node.id); }}>
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
              onClick={() => { if (props.selectedNodeId === null) handleNodeSelection("default"); }}
            >
              {defaultContextWindowLabel}
            </SegmentedControlItem>
          </SegmentedControl>
          {selectedNodeIsInvalid ? <p className="vk-stats-note">{formatContextWindowNodeTokens(selectedNode.tokens)} · {t("composer.contextWindow.invalid")}</p> : null}
          {nodeError !== null ? <div className="vk-ctx-failure">
            <p role="alert">{t("composer.contextWindow.saveFailed")}</p>
            {nodeError ? <details>
              <summary>{t("composer.capability.errorDetails")}</summary>
              <pre tabIndex={0}>{nodeError}</pre>
            </details> : null}
          </div> : null}
        </div>
      </PopoverContent>
    </Popover>
  );
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
      className="vk-ctx-track"
      role="progressbar"
      aria-valuemax={100}
      aria-valuemin={0}
      aria-valuenow={percent}
    >
      <i className="vk-ctx-zone" data-at="90" />
      <i className="vk-ctx-zone" data-at="95" />
      <div className="vk-ctx-fill" style={{ width: `${percent}%` }}>
        {props.breakdown.length > 0
          ? props.breakdown.map((item) => (
              <span
                data-ctx-kind={item.kind}
                key={item.kind}
                style={{ width: `${item.basisPoints / 100}%` }}
              />
            ))
          : null}
      </div>
      <i className="vk-ctx-mark" data-at="90" />
      <i className="vk-ctx-mark" data-at="95" />
      <i className="vk-ctx-mark" data-at="100" />
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

// CONTEXT_USAGE_KIND_KEYS maps service category kinds to localized labels.
// CONTEXT_USAGE_KIND_KEYS 将服务分类键映射为本地化标签。
const CONTEXT_USAGE_KIND_KEYS: Record<string, TranslationKey> = {
  messages: "composer.contextUsage.kind.messages",
  "system-tools": "composer.contextUsage.kind.system-tools",
  "mcp-tools": "composer.contextUsage.kind.mcp-tools",
  other: "composer.contextUsage.kind.other",
  skills: "composer.contextUsage.kind.skills",
  "system-prompt": "composer.contextUsage.kind.system-prompt",
};

// contextUsageKindLabel localizes known kinds and keeps the service label for unknown ones.
// contextUsageKindLabel 本地化已知分类，未知分类保留服务标签。
function contextUsageKindLabel(item: ContextUsageBreakdownItem, t: (key: TranslationKey) => string): string {
  const key = CONTEXT_USAGE_KIND_KEYS[item.kind];
  return key ? t(key) : item.label;
}

// formatContextUsageTotal renders the compact current-window summary.
// formatContextUsageTotal 渲染紧凑的当前窗口摘要。
function formatContextUsageTotal(snapshot: ContextUsageSnapshot, locale: string): string {
  const usageBasisPoints = contextUsageBasisPoints(snapshot);
  if (typeof snapshot.usedTokens !== "number" || !Number.isFinite(snapshot.usedTokens)) {
    return formatBasisPoints(usageBasisPoints);
  }
  const used = formatTokenCount(snapshot.usedTokens);
  if (typeof snapshot.maxTokens !== "number" || !Number.isFinite(snapshot.maxTokens)) {
    return `${used} ${tokenUnitLabel(locale)}`;
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
export function ComposerAiCapabilityControl(props: ComposerAiCapabilityControlProps) {
  // Provider tools are frozen with a dedicated session, so the editor is not offered.
  // 供应商工具随专职会话冻结，因此不提供编辑器。
  const dedicated = useDedicatedComposer();
  const { onConflictChange } = props;
  // The editor may have reported "not ready" before the session was known to be dedicated; hiding it must
  // release that block, or the first composer after creation could never send.
  // 编辑器可能在得知会话为专职之前已报告“未就绪”；隐藏编辑器时必须解除该阻塞，否则创建后的首个输入区将无法发送。
  React.useEffect(() => {
    if (dedicated) onConflictChange?.(false);
  }, [dedicated, onConflictChange]);
  if (dedicated) return null;
  return <AiCapabilityControlEditor {...props} />;
}

/** Inputs of the composer AI capability control.
 * 输入区 AI 能力控件的输入。 */
type ComposerAiCapabilityControlProps = {
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
};

/** Capability editor body for ordinary sessions and drafts.
 * 普通会话与草稿使用的能力编辑器主体。 */
function AiCapabilityControlEditor(props: ComposerAiCapabilityControlProps) {
  const { t } = useI18n();
  const [open, setOpen] = React.useState(false);
  // summaryOpen controls the read-only hover summary; it never opens while the editor dialog is open.
  // summaryOpen 控制只读悬浮摘要；编辑对话框打开期间绝不打开。
  const [summaryOpen, setSummaryOpen] = React.useState(false);
  // summarySuppressed skips the focus-restoration open after the dialog closes until the next hover or blur.
  // summarySuppressed 在对话框关闭后跳过焦点恢复引发的打开，直到下一次悬停或失焦。
  const summarySuppressed = React.useRef(false);
  // lastCapabilityByTab remembers the capability viewed in each tab so switching back restores it.
  // lastCapabilityByTab 记住每个标签中查看过的能力，切回时恢复。
  const lastCapabilityByTab = React.useRef<Partial<Record<string, PublicAiProviderToolCapabilityKind>>>({});
  // Recovery actions remove their own button; retain keyboard focus on the next recovery or current category.
  // 恢复操作会移除自身按钮；将键盘焦点保留在下一个恢复操作或当前分类。
  const recoveryFocusPending = React.useRef(false);
  const extensionRecoveryList = React.useRef<HTMLDivElement>(null);
  const selectedCategoryButton = React.useRef<HTMLButtonElement>(null);
  // Retry focus remains within the recovered controls after the loading branch removes the clicked button.
  // 加载分支移除重试按钮后，将焦点保留在恢复后的控件中。
  const retryFocusPending = React.useRef(false);
  const retryButton = React.useRef<HTMLButtonElement>(null);
  // settings owns the session draft, authoritative loads and guarded saves; focus handling stays with the dialog.
  // settings 持有会话草稿、权威读取与受保护保存；焦点处理保留在对话框中。
  const settings = useComposerCapabilitySettings({
    conversationId: props.conversationId,
    draftSettings: props.draftSettings,
    onDraftSettingsChange: props.onDraftSettingsChange,
    onConflictChange: props.onConflictChange,
    open,
    setOpen,
  });
  const {
    selectedCapability, setSelectedCapability, draft, setDraft, saving, error, loadState, setReloadVersion,
    mutationPending, loadStatus, settingsReady, capabilities, conflicts, enabledCount, hasUnsavedChanges,
    defaultSettings, draftMatchesDefaults, restoreDefaults, save, rebuild, summaryItems, describeCapability,
    selectedDescription, contextBreakPending, setContextBreakPending,
  } = settings;
  React.useLayoutEffect(() => {
    if (!recoveryFocusPending.current) return;
    recoveryFocusPending.current = false;
    const nextRecovery = extensionRecoveryList.current?.querySelector<HTMLButtonElement>('[data-extension-recovery]');
    (nextRecovery ?? selectedCategoryButton.current)?.focus();
  }, [draft]);
  React.useLayoutEffect(() => {
    if (!retryFocusPending.current || !open || loadStatus === "loading") return;
    retryFocusPending.current = false;
    (loadStatus === "failed" ? retryButton.current : selectedCategoryButton.current)?.focus();
  }, [open, loadStatus]);

  // capabilityTabs presents the shared intent groups as page tabs; capabilities no group declares yet get a trailing tab.
  // capabilityTabs 将共享意图分组呈现为页面标签；尚未归组的能力放在末尾标签中。
  const capabilityTabs = groupProviderToolCapabilities(capabilities).map(({ group, capabilities: members }) => ({
    id: group?.id ?? "other",
    label: t(group ? group.labelKey : "composer.capability.group.other"),
    capabilities: members,
  }));
  // activeCapabilityTab is derived from the selected capability, so selection and tab can never disagree.
  // activeCapabilityTab 由所选能力推导，因此选择与标签永远一致。
  const activeCapabilityTab = capabilityTabs.find((tab) => tab.capabilities.includes(selectedCapability))
    ?? capabilityTabs[0];
  // selectCapabilityTab returns to the capability last viewed in that tab, or its first capability.
  // selectCapabilityTab 回到该标签上次查看的能力，否则选中其第一项。
  // Parameters: id is the clicked tab; returns nothing.
  // 参数：id 是被点击的标签；无返回值。
  const selectCapabilityTab = (id: string): void => {
    const tab = capabilityTabs.find((candidate) => candidate.id === id);
    if (!tab || !activeCapabilityTab || tab.id === activeCapabilityTab.id) return;
    lastCapabilityByTab.current[activeCapabilityTab.id] = selectedCapability;
    const remembered = lastCapabilityByTab.current[tab.id];
    setSelectedCapability(remembered && tab.capabilities.includes(remembered) ? remembered : tab.capabilities[0]);
  };

  return (
    <Dialog open={open} onOpenChange={(nextOpen) => {
      if (mutationPending.current) return;
      setOpen(nextOpen);
      if (nextOpen) setSummaryOpen(false);
      else summarySuppressed.current = true;
    }}>
      <HoverCard
        onOpenChange={(nextOpen) => {
          if (nextOpen && (open || summarySuppressed.current)) return;
          setSummaryOpen(nextOpen);
        }}
        open={summaryOpen}
      >
        <HoverCardTrigger asChild>
          <DialogTrigger asChild>
            <button
              aria-label={props.ariaLabel}
              className="vk-pick"
              onBlur={() => { summarySuppressed.current = false; }}
              onPointerEnter={() => { summarySuppressed.current = false; }}
              type="button"
            >
              <span className="vk-pick-icon" data-tone={loadStatus !== "loading" && (loadStatus === "failed" || conflicts.length) ? "warn" : undefined}>
                {loadStatus === "loading" ? <LoaderCircle size={15} className="vk-spin" />
                  : loadStatus === "failed" || conflicts.length ? <AlertTriangle size={15} />
                    : <SlidersHorizontal size={15} />}
              </span>
              {settingsReady && enabledCount > 0 ? (
                <span aria-hidden="true" className="vk-pick-count">{enabledCount}</span>
              ) : null}
              <ChevronDown aria-hidden="true" className="vk-pick-chevron" />
            </button>
          </DialogTrigger>
        </HoverCardTrigger>
        <HoverCardContent align="start" className="vk-cap-sum-pop" side="top" sideOffset={8}>
          <ComposerCapabilitySummary
            items={summaryItems}
            onConfigure={() => {
              setSummaryOpen(false);
              setOpen(true);
            }}
            status={loadStatus}
          />
        </HoverCardContent>
      </HoverCard>
      <DialogContent className="vk-cap-dialog" layout="document" scrollMode="fixed-footer" size="wide">
        {/* Title and effect hint share one line so the intent tabs and list start right under the header. */}
        {/* 标题与生效提示共用一行，让意图标签和列表紧贴标题下方。 */}
        <div className="vk-cap-head">
          <DialogHeader>
            <DialogTitle>{t("composer.capability.dialogTitle")}</DialogTitle>
            <DialogDescription>{t("composer.capability.dialogDescription")}</DialogDescription>
          </DialogHeader>
          {/* Intent tabs close the header so their divider runs edge to edge above both panes. */}
          {/* 意图标签收尾标题区，使其分隔线贯通两侧面板上方。 */}
          {settingsReady && activeCapabilityTab ? (
            <nav aria-label={t("composer.capability.dialogTitle")} className="vk-tabs vk-cap-tabs" data-variant="line">
              {capabilityTabs.map((tab) => {
                const described = tab.capabilities.map(describeCapability);
                const conflicted = described.some((description) => description.conflict);
                const active = tab.id === activeCapabilityTab.id;
                return (
                  <button
                    aria-current={active ? "page" : undefined}
                    className="vk-tab"
                    data-state={active ? "active" : undefined}
                    disabled={saving}
                    key={tab.id}
                    onClick={() => selectCapabilityTab(tab.id)}
                    type="button"
                  >
                    <span>{tab.label}</span>
                    <span className="vk-cap-tab-count" data-conflict={conflicted || undefined}>
                      {conflicted ? <AlertTriangle aria-hidden="true" size={12} /> : null}
                      {conflicted ? <span className="vk-sr-only">{t("composer.capability.conflict.tooltip")}</span> : null}
                      {described.filter((description) => description.enabled).length}/{tab.capabilities.length}
                    </span>
                  </button>
                );
              })}
            </nav>
          ) : null}
        </div>
        <div className="vk-cap-body">
        {loadState.conversationId === props.conversationId && error
          ? <div className="vk-cap-alert">
              <p role="alert">
                {t(loadStatus === "failed" ? "composer.capability.loadFailed" : "composer.capability.operationFailed")}
              </p>
              <details>
                <summary>{t("composer.capability.errorDetails")}</summary>
                <pre>{error}</pre>
              </details>
            </div> : null}
        {loadStatus === "loading" ? (
          <div className="vk-cap-grid">
            <StreamPlaceholder className="vk-cap-list" label={t("composer.capability.loading")} shape="rows" count={6} />
            <div>
              <StreamPlaceholder decorative shape="fields" count={1} />
              <StreamPlaceholder decorative shape="rows" count={4} />
            </div>
          </div>
        ) : null}
        {loadStatus === "failed" ? (
          <div className="vk-cap-retry">
            {props.conversationId ? <p className="vk-cap-note">{t("composer.capability.rebuildHint")}</p> : null}
            <div>
              <Button type="button" variant="outline" ref={retryButton} disabled={saving} onClick={() => {
                retryFocusPending.current = true;
                setReloadVersion((current) => current + 1);
              }}>
                {t("composer.capability.retry")}
              </Button>
              {props.conversationId ? <Button type="button" variant="outline" disabled={saving} onClick={() => void rebuild()}>
                {t(saving ? "composer.capability.rebuilding" : "composer.capability.rebuild")}
              </Button> : null}
            </div>
          </div>
        ) : null}
        {settingsReady && activeCapabilityTab ? <div className="vk-cap-grid">
          <ComposerCapabilityKindList
            selectedCategoryButton={selectedCategoryButton}
            settings={settings}
            tab={activeCapabilityTab}
          />
          <section>
            {selectedDescription ? (
              <header className="vk-cap-detail-head">
                <span
                  aria-hidden="true"
                  className="vk-cap-glyph"
                  data-off={selectedDescription.enabled ? undefined : ""}
                  data-tile=""
                  style={{ "--vk-cap-color": providerToolCapabilityColor(selectedCapability) } as React.CSSProperties}
                >
                  <ProviderToolCapabilityIcon capability={selectedCapability} size={17} />
                </span>
                <span className="vk-cap-detail-heading">
                  <strong>{providerToolCapabilityLabel(selectedCapability, t)}</strong>
                  <small>{selectedDescription.subtitle}</small>
                </span>
                <span className="vk-cap-state" data-state={selectedDescription.conflict ? "conflict" : selectedDescription.enabled ? "on" : "off"}>
                  {selectedDescription.conflict
                    ? t("composer.capability.conflict.tooltip")
                    : t(selectedDescription.enabled ? "settings.providerTools.enabled" : "settings.providerTools.disabled")}
                </span>
              </header>
            ) : null}
            {isMediaUnderstandingCapability(selectedCapability)
              && draft.enabledCapabilities.includes(selectedCapability)
              && conflicts.some((conflict) => conflict.capability === selectedCapability) ? (
              <div className="vk-cap-recover-bar">
                <span>{t("composer.capability.unavailableEnabled")}</span>
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
              <ComposerCapabilityMediaRoute capability={selectedCapability} settings={settings} />
            ) : isModelExtensionCapability(selectedCapability) ? (
              <ComposerCapabilityExtensionList
                extensionRecoveryList={extensionRecoveryList}
                recoveryFocusPending={recoveryFocusPending}
                settings={settings}
              />
            ) : isDirectProviderToolCapability(selectedCapability) ? (
              <ComposerCapabilityDirectRoute settings={settings} />
            ) : (
              <div className="vk-cap-none">
                {t("composer.capability.empty.noTargets")}
              </div>
            )}
          </section>
        </div> : null}
        </div>
        <DialogFooter className="vk-cap-foot">
          <span className="vk-cap-foot-hint">
            {!settingsReady
              ? loadStatus === "failed" ? t("composer.capability.loadFailed") : <InlineStreamPlaceholder decorative />
              : hasUnsavedChanges
              ? t("composer.capability.saveHint.unsaved")
              : t("composer.capability.saveHint.clean")}
          </span>
          <Button
            disabled={!defaultSettings || saving || draftMatchesDefaults}
            onClick={restoreDefaults}
            title={t("composer.capability.restoreDefaults.hint").replace("{page}", t("settings.aiCapabilities"))}
            type="button"
            variant="ghost"
          >
            <RotateCcw aria-hidden="true" size={13} />
            {t("composer.capability.restoreDefaults")}
          </Button>
          <Button
            disabled={!settingsReady || saving || !hasUnsavedChanges || conflicts.length > 0}
            onClick={() => void save()}
            type="button"
            variant="primary"
          >
            {saving && settingsReady
              ? t("composer.capability.save.saving")
              : t("composer.capability.save")}
          </Button>
        </DialogFooter>
        {/* Changing tools after the first request breaks context; the user confirms before it is saved. */}
        {/* 首次请求后修改工具会打破上下文；保存前由用户确认。 */}
        <AlertDialog open={contextBreakPending} onOpenChange={setContextBreakPending}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t("composer.capability.contextBreak.title")}</AlertDialogTitle>
              <AlertDialogDescription>{t("composer.capability.contextBreak.description")}</AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{t("composer.capability.close.keepEditing")}</AlertDialogCancel>
              <AlertDialogAction onClick={() => void save(true)}>
                {t("composer.capability.contextBreak.confirm")}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </DialogContent>
    </Dialog>
  );
}
