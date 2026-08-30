import React from "react";
import {
  AlertTriangle,
  FileSearch,
  Image,
  LoaderCircle,
  LockKeyhole,
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
import type {
  ComposerProviderToolSettingsSnapshot,
  ComposerSelectOption,
} from "@/components/composer/composer-model";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import {
  isDirectProviderToolCapability,
  isModelExtensionCapability,
} from "@/lib/provider-tool-capabilities";
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
  PublicAiModelCapabilities,
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
  // providerUsageTotal uses only the cumulative total derived from canonical V1 API facts.
  // providerUsageTotal 仅使用从规范 V1 API 事实派生的累计总量。
  const providerUsageTotal = providerUsage?.cumulativeUsage?.totalTokens ?? providerUsage?.lastUsage?.totalTokens;
  // providerUsageUnknown keeps missing Provider usage explicit in the diagnostic row.
  // providerUsageUnknown 在 Provider 用量缺失时保持诊断行显式可见。
  const providerUsageUnknown = t("conversation.process.providerUsageUnknown");
  // providerUsageTitle exposes the one canonical session scope without synthetic branch or billing scopes.
  // providerUsageTitle 展示唯一规范会话口径，不构造分支或计费统计口径。
  const providerUsageTitle = providerUsage
    ? t("conversation.process.providerUsageSession")
        .replace("{calls}", formatCompactProviderCount(providerUsage.providerCallCount, providerUsageUnknown))
        .replace("{tokens}", formatCompactProviderCount(providerUsageTotal, providerUsageUnknown))
        .replace("{unknown}", String(providerUsage.unknownUsageCallCount))
    : undefined;
  // providerUsageAriaLabel exposes the two compact metrics through the capacity control.
  // providerUsageAriaLabel 通过上下文容量控件公开两项紧凑统计。
  const providerUsageAriaLabel = providerUsage
    ? `${t("composer.contextUsage.totalRequests")}: ${formatCompactProviderCount(providerUsage.providerCallCount, providerUsageUnknown)} · ${t("composer.contextUsage.totalTokens")}: ${formatCompactProviderCount(providerUsageTotal, providerUsageUnknown)}`
    : null;
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
    providerUsageAriaLabel,
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
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          aria-label={contextUsageAriaLabel}
          className={cn("context-usage-control", selectedNode && "is-locked")}
          type="button"
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
          <span>{formatContextUsageTotal(snapshot, locale)}</span>
        </div>
        <ContextUsageProgressBar breakdown={breakdown} usageBasisPoints={usageBasisPoints} />
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
          <span>{t("composer.contextUsage.cacheHitRate")}</span>
          <strong>
            {snapshot.cacheHitBasisPoints == null
              ? t("composer.contextUsage.noData")
            : formatBasisPoints(snapshot.cacheHitBasisPoints)}
          </strong>
        </div>
        {providerUsage ? (
          <div className="context-usage-provider-section" title={providerUsageTitle}>
            <div className="context-usage-provider-row">
              <div className="context-usage-provider-metric">
                <span>{t("composer.contextUsage.totalRequests")}</span>
                <strong>{formatCompactProviderCount(providerUsage.providerCallCount, providerUsageUnknown)}</strong>
              </div>
              <div className="context-usage-provider-metric">
                <span>{t("composer.contextUsage.totalTokens")}</span>
                <strong>{formatCompactProviderCount(providerUsageTotal, providerUsageUnknown)}</strong>
              </div>
            </div>
          </div>
        ) : null}
        <div className="context-usage-window-section">
          <span className="context-usage-window-title">{t("composer.contextWindow.title")}</span>
          <SegmentedControl
            aria-label={t("composer.contextWindow.title")}
            className="context-usage-window-switch"
            onValueChange={(value) => {
              const nextNodeId = contextWindowNodeIdFromSwitchValue(value, props.contextCompactionWindowNodes);
              if (nextNodeId !== undefined) {
                void props.onNodeChange(nextNodeId);
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
              <SegmentedControlItem disabled value={selectedNode.id}>
                <AlertTriangle aria-hidden="true" size={13} />
                {formatContextWindowNodeTokens(selectedNode.tokens)} · {t("composer.contextWindow.invalid")}
              </SegmentedControlItem>
            ) : null}
            <SegmentedControlItem
              aria-label={`${t("composer.contextWindow.default")} · ${defaultContextWindowLabel}`}
              value="default"
            >
              {defaultContextWindowLabel}
            </SegmentedControlItem>
          </SegmentedControl>
        </div>
      </TooltipContent>
    </Tooltip>
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
function formatContextUsageTotal(snapshot: ContextUsageSnapshot, locale: string): string {
  const usageBasisPoints = contextUsageBasisPoints(snapshot);
  if (typeof snapshot.usedTokens !== "number" || !Number.isFinite(snapshot.usedTokens)) {
    return formatBasisPoints(usageBasisPoints);
  }
  const used = formatContextTokenCount(snapshot.usedTokens, locale);
  if (typeof snapshot.maxTokens !== "number" || !Number.isFinite(snapshot.maxTokens)) {
    return `${used} Token`;
  }
  return `${used}/${formatContextTokenCount(snapshot.maxTokens, locale)} (${formatBasisPoints(usageBasisPoints)})`;
}

// formatContextTokenCount formats large token counts with Chinese ten-thousand and hundred-million units.
// formatContextTokenCount 使用中文万与亿单位格式化较大的 Token 数量。
function formatContextTokenCount(value: number, locale = "zh-CN"): string {
  if (!locale.startsWith("zh")) {
    if (value >= 1_000_000_000) {
      return `${formatCompactDecimal(value / 1_000_000_000)}B`;
    }
    if (value >= 1_000_000) {
      return `${formatCompactDecimal(value / 1_000_000)}M`;
    }
    if (value >= 1_000) {
      return `${formatCompactDecimal(value / 1_000)}K`;
    }
    return Math.max(0, Math.round(value)).toLocaleString("en-US");
  }
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
    extensionTargetSets: {},
    mediaUnderstandingModes: {},
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
  // loadStatus invalidates the previous owner synchronously, before the next effect can start.
  // loadStatus 在下一次 effect 启动前同步使上一个会话的状态失效。
  const loadStatus = loadState.conversationId === props.conversationId ? loadState.status : "loading";
  // settingsReady is required both by the UI and the save callback to prevent destructive empty writes after read errors.
  // settingsReady 同时约束界面与保存回调，防止读取失败后用空配置覆盖原设置。
  const settingsReady = loadStatus === "ready";

  React.useEffect(() => {
    let active = true;
    saveGeneration.current += 1;
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
              mediaUnderstandingModes: sessionSettings.mediaUnderstandingModes,
            }
          : props.draftSettings ?? {
              enabledCapabilities: nextSnapshot.providerToolEnabledCapabilities,
              selections: nextSnapshot.providerToolSelections,
              extensionTargetSets: nextSnapshot.providerExtensionTargetSets,
              mediaUnderstandingModes: mediaUnderstandingModesFromSnapshot(nextSnapshot),
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

  const capabilities = settingsReady && snapshot ? orderedProviderToolCapabilities(snapshot) : [];
  const capabilityTargets = snapshot?.providerToolTargets.filter(
    (target) => target.target.contract.capability === selectedCapability
      && (!isModelExtensionCapability(selectedCapability)
        || target.manifestSourceLayer !== "custom"),
  ) ?? [];
  const selectedModels = uniqueProviderToolModels(capabilityTargets);
  // selectedExtensionTargets is the complete native model set for the selected capability.
  // selectedExtensionTargets 是当前选中能力的完整原生模型集合。
  const selectedExtensionTargets = draft.extensionTargetSets[selectedCapability] ?? [];
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
  // selectedMediaMode is the session-selected backend family, defaulting to the global active mode.
  // selectedMediaMode 是会话选择的后端类型，缺省时使用全局当前活动模式。
  const selectedMediaMode = snapshot && isMediaUnderstandingCapability(selectedCapability)
    ? draft.mediaUnderstandingModes[selectedCapability]
      ?? snapshot.mediaUnderstandingSelections?.[selectedCapability]?.mode
    : undefined;
  // selectedMediaBackendSummary describes the selected global route without exposing session model controls.
  // selectedMediaBackendSummary 描述会话模式对应的全局路由，不暴露会话级模型控件。
  const selectedMediaBackendSummary = snapshot && isMediaUnderstandingCapability(selectedCapability)
    ? mediaUnderstandingBackendSummary(snapshot, selectedCapability, t, selectedMediaMode)
    : null;
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
    if (!settingsReady || saving) {
      return;
    }
    if (!props.conversationId) {
      props.onDraftSettingsChange?.(draft);
      setBaselineSignature(providerToolSettingsSignature(draft));
      setOpen(false);
      return;
    }
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
        mediaUnderstandingModes: draft.mediaUnderstandingModes,
      });
      if (generation !== saveGeneration.current) {
        return;
      }
      const nextDraft = {
        enabledCapabilities: next.enabledCapabilities,
        selections: next.selections,
        extensionTargetSets: next.extensionTargetSets,
        mediaUnderstandingModes: next.mediaUnderstandingModes,
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
        setSaving(false);
      }
    }
  }, [
    draft,
    settingsReady,
    saving,
    props.conversationId,
    props.onDraftSettingsChange,
  ]);

  // Rebuild only the selected conversation's addon after an explicit click; ignore stale responses.
  // 仅在显式点击后重建当前会话附加项；忽略过期响应。
  // Returns after the write, then reloads authoritative settings without closing the recovery view.
  // 写入完成后返回，并重新加载权威设置，不关闭恢复视图。
  const rebuild = React.useCallback(async () => {
    if (!props.conversationId || saving) {
      return;
    }
    const generation = ++saveGeneration.current;
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
        setSaving(false);
      }
    }
  }, [props.conversationId, saving]);

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
            {!settingsReady ? t(loadStatus === "failed" ? "composer.capability.loadFailed" : "composer.capability.loading")
              : conflicts.length ? t("composer.capability.conflict.tooltip") : `${enabledCount} / ${capabilities.length}`}
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
        {loadState.conversationId === props.conversationId && error
          ? <p className="settings-muted-line settings-error-text" role="alert">{error}</p> : null}
        {loadStatus === "loading" ? <p role="status">{t("composer.capability.loading")}</p> : null}
        {loadStatus === "failed" ? (
          <div>
            {props.conversationId ? <p className="settings-muted-line">{t("composer.capability.rebuildHint")}</p> : null}
            <div className="flex gap-2">
              <button className="skill-action-button" type="button" disabled={saving} onClick={() => setReloadVersion((current) => current + 1)}>
                {t("composer.capability.retry")}
              </button>
              {props.conversationId ? <button className="skill-action-button" type="button" disabled={saving} onClick={() => void rebuild()}>
                {t(saving ? "composer.capability.rebuilding" : "composer.capability.rebuild")}
              </button> : null}
            </div>
          </div>
        ) : null}
        {settingsReady && conflicts.length ? (
          <div className="composer-capability-conflict-banner">
            <AlertTriangle aria-hidden="true" size={16} />
            <span>{t("composer.capability.conflict.banner")}</span>
          </div>
        ) : null}
        {settingsReady ? <div className="composer-capability-dialog-grid">
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
              const enabled = directMode
                ? draft.enabledCapabilities.includes(capability)
                : extensionSelectionCount > 0;
              const targets = snapshot?.providerToolTargets.filter(
                (target) => target.target.contract.capability === capability
                  && (!isModelExtensionCapability(capability)
                    || target.manifestSourceLayer !== "custom"),
              ) ?? [];
              // mediaBackendOptions exposes only the backend families configured globally for this capability.
              // mediaBackendOptions 只暴露该能力已在全局配置的后端类型。
              const mediaBackendOptions = snapshot?.mediaUnderstandingBackendOptions?.[capability];
              const mediaBackendSummary = snapshot && isMediaUnderstandingCapability(capability)
                ? mediaUnderstandingBackendSummary(
                  snapshot,
                  capability,
                  t,
                  draft.mediaUnderstandingModes[capability]
                    ?? snapshot.mediaUnderstandingSelections?.[capability]?.mode,
                )
                : null;
              const mediaBackendModeCount = Number(mediaBackendOptions?.provider_tool !== undefined)
                + Number(mediaBackendOptions?.model_replacement !== undefined);
              // targetCount reports configured media backend modes or the regular selectable model count.
              // targetCount 对媒体能力报告已配置后端类型数量，否则报告常规可选模型数量。
              const targetCount = isMediaUnderstandingCapability(capability)
                ? mediaBackendModeCount
                : uniqueProviderToolModels(targets).length;
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
                  {directMode ? (
                    <input
                      checked={enabled}
                      disabled={providerToolCapabilitySwitchDisabled(saving, targetCount, enabled)}
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
                            return { ...current, enabledCapabilities: Array.from(enabledCapabilities).sort() };
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
            {isMediaUnderstandingCapability(selectedCapability) ? (
              selectedMediaBackendSummary ? (
                <div className="composer-capability-route-selectors">
                  <label>
                    <span>{t("composer.capability.mediaBackendMode")}</span>
                    <Select
                      disabled={saving}
                      value={selectedMediaBackendSummary.mode}
                      onValueChange={(value) => {
                        const mode = value as PublicAiMediaUnderstandingBackendMode;
                        setDraft((current) => ({
                          ...current,
                          mediaUnderstandingModes: {
                            ...current.mediaUnderstandingModes,
                            [selectedCapability]: mode,
                          },
                        }));
                      }}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {selectedMediaBackendSummary.mode === "provider_tool"
                          && !snapshot?.mediaUnderstandingBackendOptions?.[selectedCapability]?.provider_tool ? (
                          <SelectItem disabled value="provider_tool">
                            {t("composer.capability.mode.provider")}
                          </SelectItem>
                        ) : null}
                        {snapshot?.mediaUnderstandingBackendOptions?.[selectedCapability]?.provider_tool ? (
                          <SelectItem value="provider_tool">
                            {t("composer.capability.mode.provider")}
                          </SelectItem>
                        ) : null}
                        {selectedMediaBackendSummary.mode === "model_replacement"
                          && !snapshot?.mediaUnderstandingBackendOptions?.[selectedCapability]?.model_replacement ? (
                          <SelectItem disabled value="model_replacement">
                            {t("composer.capability.mode.model")}
                          </SelectItem>
                        ) : null}
                        {snapshot?.mediaUnderstandingBackendOptions?.[selectedCapability]?.model_replacement ? (
                          <SelectItem value="model_replacement">
                            {t("composer.capability.mode.model")}
                          </SelectItem>
                        ) : null}
                      </SelectContent>
                    </Select>
                  </label>
                  <label>
                    <span>{t("composer.capability.mediaFixedRoute")}</span>
                    <strong>{selectedMediaBackendSummary.routeLabel}</strong>
                  </label>
                  <p className="composer-capability-media-note settings-muted-line">
                    {t("composer.capability.mediaBackendNote")}
                  </p>
                </div>
              ) : (
                <div className="composer-capability-dialog-empty">
                  {t("composer.capability.mediaBackendMissing")}
                </div>
              )
            ) : selectedModels.length && isModelExtensionCapability(selectedCapability) ? (
              <div className="composer-capability-extension-list">
                <p className="settings-muted-line">
                  {t("composer.capability.extensionHint")}
                </p>
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
                    <article className="composer-capability-extension-row" key={`${model.target.provider_id}/${model.target.upstream_target_id}`}>
                      <input
                        checked={Boolean(selectedExtension)}
                        disabled={saving || !profile}
                        onChange={(event) => {
                          const retained = selectedExtensionTargets.filter((selection) => !modelTargets.some((target) => (
                            target.connectionId === selection.connection_id
                            && target.target.target_id === selection.target_id
                          )));
                          if (event.currentTarget.checked && selectedExtensionTarget) {
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
                        type="checkbox"
                      />
                      <span className="composer-capability-extension-copy">
                        <strong>{model.target.display_name}</strong>
                        <small>{profile?.summary ?? t("composer.capability.extensionProfileMissing")}</small>
                        {profile?.constraints?.length ? (
                          <small>{t("composer.capability.extensionConstraints").replace("{value}", profile.constraints.join(t("global.listDelimiter")))}</small>
                        ) : null}
                        {profile?.limitations?.length ? (
                          <small>{t("composer.capability.extensionLimitations").replace("{value}", profile.limitations.join(t("global.listDelimiter")))}</small>
                        ) : null}
                      </span>
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
                    </article>
                  );
                })}
              </div>
            ) : selectedModels.length ? (
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
        </div> : null}
        <DialogFooter className="composer-capability-dialog-footer">
          <span className="composer-capability-dialog-save-hint">
            {!settingsReady
              ? t(loadStatus === "failed" ? "composer.capability.loadFailed" : "composer.capability.loading")
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
      Object.entries(settings.mediaUnderstandingModes)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([capability, mode]) => [capability, mode ?? ""]),
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

// orderedProviderToolCapabilities merges provider-native targets with model-replacement-only media backends.
// orderedProviderToolCapabilities 合并供应商原生目标与仅由模型代替提供的媒体后端。
export function orderedProviderToolCapabilities(
  snapshot: PublicAiSettingsSnapshot,
): PublicAiProviderToolCapabilityKind[] {
  // mediaCapabilities keeps configured replacement backends visible even without a provider-tool target.
  // mediaCapabilities 让已配置的模型代替后端在没有供应商工具目标时仍然可见。
  const mediaCapabilities = Array.from(new Set([
    ...Object.keys(snapshot.mediaUnderstandingSelections ?? {}),
    ...Object.keys(snapshot.mediaUnderstandingBackendOptions ?? {}),
  ]))
    .filter(isMediaUnderstandingCapability);
  return orderProviderToolCapabilities(
    [
      ...snapshot.providerToolTargets.map((target) => target.target.contract.capability),
      ...mediaCapabilities,
    ],
  );
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

// mediaUnderstandingModesFromSnapshot derives the default session mode from the active global route.
// mediaUnderstandingModesFromSnapshot 从全局活动路由派生新会话的默认后端模式。
function mediaUnderstandingModesFromSnapshot(
  snapshot: PublicAiSettingsSnapshot,
): Partial<Record<PublicAiMediaUnderstandingCapabilityKind, PublicAiMediaUnderstandingBackendMode>> {
  return Object.entries(snapshot.mediaUnderstandingSelections ?? {}).reduce(
    (result, [capability, backend]) => {
      if (isMediaUnderstandingCapability(capability)) {
        result[capability] = backend.mode;
      }
      return result;
    },
    {} as Partial<Record<PublicAiMediaUnderstandingCapabilityKind, PublicAiMediaUnderstandingBackendMode>>,
  );
}

// mediaUnderstandingBackendSummary resolves one fixed backend and its current executable state.
// mediaUnderstandingBackendSummary 解析一个固定后端及其当前可执行状态。
function mediaUnderstandingBackendSummary(
  snapshot: PublicAiSettingsSnapshot,
  capability: PublicAiProviderToolCapabilityKind,
  // t is omitted only by the pure conflict validator, which consumes availability and never renders mode labels.
  // t 仅在纯冲突校验器中省略；该校验器只消费可用性，不渲染模式标签。
  t?: ReturnType<typeof useI18n>["t"],
  requestedMode?: PublicAiMediaUnderstandingBackendMode,
): {
  mode: PublicAiMediaUnderstandingBackendMode;
  modeLabel: string;
  routeLabel: string;
  available: boolean;
} | null {
  // activeBackend remains the legacy/global active choice used when a session has no mode override.
  // activeBackend 是会话没有模式覆盖时使用的旧版/全局活动选择。
  const activeBackend = snapshot.mediaUnderstandingSelections?.[capability];
  const mode = requestedMode ?? activeBackend?.mode;
  if (!mode) {
    return null;
  }
  // backend resolves the exact independent route stored for the selected session mode.
  // backend 解析会话选定模式中保存的精确独立路由。
  const options = snapshot.mediaUnderstandingBackendOptions?.[capability];
  const backend: PublicAiMediaUnderstandingBackendSelection | null = mode === "provider_tool"
    ? options?.provider_tool
      ? { mode, selection: options.provider_tool }
      : activeBackend?.mode === mode
        ? activeBackend
        : null
    : options?.model_replacement
      ? {
          mode,
          selection: options.model_replacement.selection,
          reasoning_effort: options.model_replacement.reasoning_effort,
        }
      : activeBackend?.mode === mode
        ? activeBackend
        : null;
  if (!backend) {
    // unavailableRoute preserves the selected mode as a recoverable session choice when its global route was removed.
    // unavailableRoute 在全局路由被移除时保留会话模式，使用户仍可切换到其他可用模式。
    return {
      mode,
      modeLabel: t
        ? mode === "provider_tool"
          ? t("composer.capability.mode.provider")
          : t("composer.capability.mode.model")
        : "",
      routeLabel: t ? t("composer.capability.mediaBackendMissing") : "",
      available: false,
    };
  }
  if (backend.mode === "provider_tool") {
    // target resolves the exact persisted account and target without substituting another account.
    // target 精确解析持久化账号与目标，不替换为其他账号。
    const target = snapshot.providerToolTargets.find((candidate) => (
      candidate.connectionId === backend.selection.connection_id
      && candidate.target.target_id === backend.selection.target_id
      && candidate.target.contract.capability === capability
    ));
    return {
      mode: backend.mode,
      modeLabel: t ? t("composer.capability.mode.provider") : "",
      routeLabel: target
        ? `${target.target.display_name} · ${target.connectionDisplayName}`
        : `${backend.selection.target_id} · ${backend.selection.connection_id}`,
      available: Boolean(target),
    };
  }
  // candidate resolves the exact provider-model-account route selected by settings.
  // candidate 解析设置选择的精确供应商、模型与账号路由。
  const candidate = snapshot.mediaUnderstandingModelCandidates?.find((entry) => (
    entry.selection.provider_id === backend.selection.provider_id
    && entry.selection.model_id === backend.selection.model_id
    && entry.selection.connection_id === backend.selection.connection_id
  ));
  // candidateEnabled applies the one shared provider-model admission switch.
  // candidateEnabled 应用唯一共享的供应商模型准入开关。
  const candidateEnabled = snapshot.multimodalUnderstandingCandidateModels?.some((entry) => (
    entry.providerId === backend.selection.provider_id
    && entry.modelId === backend.selection.model_id
  )) === true;
  // requiredResource maps the fixed facade to its only accepted non-text resource.
  // requiredResource 将固定外壳映射到其唯一接受的非文本资源。
  const requiredResource = capability === "image_understand"
    ? "image"
    : capability === "video_understand"
      ? "video"
      : "audio";
  // reasoningAvailable rejects a retained effort that disappeared from the exact model route.
  // reasoningAvailable 拒绝已从精确模型路由中消失的持久化思考级别。
  const reasoningAvailable = candidate !== undefined
    && mediaReplacementReasoningIsAvailable(candidate.capabilities, backend.reasoning_effort);
  // candidateAvailable mirrors the backend media, conversation, structured-output, and reasoning gates.
  // candidateAvailable 镜像后端的媒体、会话、结构化输出与思考级别门槛。
  const candidateAvailable = candidateEnabled
    && candidate !== undefined
    && candidate.capabilities.operations?.includes("conversation_respond") === true
    && candidate.capabilities.input_resources?.includes(requiredResource) === true
    && candidate.capabilities.structured_output !== "unsupported"
    && reasoningAvailable;
  // reasoningLabel renders only the explicitly selected effort.
  // reasoningLabel 仅渲染显式选择的思考级别。
  const reasoningLabel = t && backend.reasoning_effort
    ? ` · ${t("composer.capability.reasoning").replace("{effort}", backend.reasoning_effort)}`
    : "";
  return {
    mode: backend.mode,
    modeLabel: t ? t("composer.capability.mode.model") : "",
    routeLabel: candidate
      ? `${candidate.providerDisplayName} · ${candidate.modelDisplayName} · ${candidate.connectionDisplayName}${reasoningLabel}`
      : `${backend.selection.provider_id}/${backend.selection.model_id} · ${backend.selection.connection_id}${reasoningLabel}`,
    available: candidateAvailable,
  };
}

// mediaReplacementReasoningIsAvailable mirrors the backend's closed reasoning-control validation.
// mediaReplacementReasoningIsAvailable 镜像后端封闭的思考控制校验。
function mediaReplacementReasoningIsAvailable(
  capabilities: PublicAiModelCapabilities,
  effort: PublicAiReasoningEffort | undefined,
): boolean {
  if (effort === undefined) {
    return true;
  }
  // supportedEfforts is the exact route-declared selectable effort set.
  // supportedEfforts 是精确路由声明的可选思考档位集合。
  const supportedEfforts = capabilities.reasoning_efforts ?? [];
  switch (capabilities.reasoning_control) {
    case "fixed_off":
      return effort === "none";
    case "fixed_on":
      return effort !== "none" && supportedEfforts.includes(effort);
    case "toggle":
    case "effort_selection":
    case "provider_adaptive":
      return supportedEfforts.includes(effort);
    default:
      return false;
  }
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
      // mediaMode is the only session-owned media backend identity; model and account remain global.
      // mediaMode 是会话唯一持有的媒体后端身份；模型与账号仍由全局配置持有。
      const mediaMode = settings.mediaUnderstandingModes[capability];
      if (!mediaMode) {
        conflicts.push({
          capability,
          kind: "missing_selection",
          message: `Capability ${capability} has no selected media-understanding backend mode.`,
        });
        continue;
      }
      const backendSummary = mediaUnderstandingBackendSummary(snapshot, capability, undefined, mediaMode);
      if (!backendSummary || !backendSummary.available) {
        conflicts.push({
          capability,
          kind: "target_unavailable",
          message: `The selected media-understanding backend mode for ${capability} is no longer available.`,
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
function uniqueProviderToolModels(
  targets: PublicAiProviderToolTargetSnapshot[],
): PublicAiProviderToolTargetSnapshot[] {
  const models = new Map<string, PublicAiProviderToolTargetSnapshot>();
  for (const target of targets) {
    // modelKey prevents collisions across providers while grouping account-expanded targets.
    // modelKey 在折叠账号展开目标时防止不同供应商之间发生冲突。
    const modelKey = `${target.target.provider_id}/${target.target.upstream_target_id}`;
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
function providerToolCapabilityColor(capability: PublicAiProviderToolCapabilityKind): string {
  if (capability === "web_search" || capability === "web_extract") {
    return "#0ea5e9";
  }
  if (
    capability === "image_understand"
    || capability === "video_understand"
    || capability === "audio_understand"
  ) {
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
  if (capability === "image_understand" || capability === "image_generate" || capability === "image_edit") {
    return <Image size={size} />;
  }
  if (capability === "video_generate" || capability === "video_edit" || capability === "video_extend") {
    return <Video size={size} />;
  }
  if (capability === "video_understand") {
    return <Video size={size} />;
  }
  if (capability === "audio_understand") {
    return <Volume2 size={size} />;
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
      <SelectContent className="composer-select-content" placement="top">
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
