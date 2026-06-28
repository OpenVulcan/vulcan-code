import * as React from "react"
import {
  Archive,
  Cpu,
  FileText,
  GitCommitHorizontal,
  PackagePlus,
  RefreshCw,
  Route,
  RotateCcw,
  Save,
  ScrollText,
  SlidersHorizontal,
} from "lucide-react"

import { ModelPickerControl, modelPickerChoiceKey } from "@/components/composer/model-picker-control"
import { PanelTitle, StatusPill } from "@/components/shared/status"
import { Badge } from "@/components/ui/badge"
import { useI18n } from "@/i18n"
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types"
import type {
  ComposerAttachmentCapabilitySummary,
  ComposerSlashCommandSummary,
  ConversationSummary,
  ProjectSummary,
  RtkGainPeriodStats,
  RtkGainStatsResponse,
  WorkbenchSnapshot,
} from "@/types"
import type { GitCommitSummaryModelSelectionSaveRequest } from "@/components/features/settings/settings-action-types"

// DEFAULT_COMMIT_BRANCH_PREFIX mirrors the service default for empty Git commit settings.
// DEFAULT_COMMIT_BRANCH_PREFIX 对齐服务端空 Git 提交设置的默认值。
const DEFAULT_COMMIT_BRANCH_PREFIX = "vulcan/"

// DEFAULT_COMMIT_SUMMARY_MODEL follows the owning conversation session model.
// DEFAULT_COMMIT_SUMMARY_MODEL 跟随所属会话模型。
const DEFAULT_COMMIT_SUMMARY_MODEL: GitCommitSummaryModelSelectionSaveRequest = { mode: "session" }

// GitCommitSettingsArea renders local Git commit dialog preferences.
// GitCommitSettingsArea 渲染 Git 提交弹窗偏好。
export function GitCommitSettingsArea(props: {
  // Service-backed settings actions.
  // 服务端设置操作。
  actions: SettingsActionProps
  // Workbench snapshot that carries the shared Git commit configuration.
  // 携带共享 Git 提交配置的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  const { locale, t } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const branchPrefix = props.workbench.gitCommit?.newBranchPrefix ?? DEFAULT_COMMIT_BRANCH_PREFIX
  const summaryModel = props.workbench.gitCommit?.summaryModel ?? DEFAULT_COMMIT_SUMMARY_MODEL
  const summaryExtraPrompt = props.workbench.gitCommit?.summaryExtraPrompt ?? ""
  const useRtkSummary = props.workbench.gitCommit?.useRtkSummary ?? true
  const summaryModelMode = summaryModel.mode
  const summaryModelProviderId = summaryModel.mode === "fixed" ? summaryModel.providerId : ""
  const summaryModelModelId = summaryModel.mode === "fixed" ? summaryModel.modelId : ""
  const [draftPrefix, setDraftPrefix] = React.useState(branchPrefix)
  const [draftSummaryModel, setDraftSummaryModel] = React.useState<GitCommitSummaryModelSelectionSaveRequest>(summaryModel)
  const [draftSummaryExtraPrompt, setDraftSummaryExtraPrompt] = React.useState(summaryExtraPrompt)
  const [saveState, setSaveState] = React.useState<"idle" | "saving" | "saved">("idle")

  React.useEffect(() => {
    setDraftPrefix(branchPrefix)
    setDraftSummaryModel(summaryModelMode === "fixed"
      ? { mode: "fixed", providerId: summaryModelProviderId, modelId: summaryModelModelId }
      : { mode: "session" })
    setDraftSummaryExtraPrompt(summaryExtraPrompt)
  }, [branchPrefix, summaryExtraPrompt, summaryModelMode, summaryModelProviderId, summaryModelModelId])

  const normalizedDraftPrefix = normalizeGitCommitBranchPrefix(draftPrefix)
  const fixedSummaryModelValue = draftSummaryModel.mode === "fixed"
    ? `${draftSummaryModel.providerId}/${draftSummaryModel.modelId}`
    : ""
  const saveDraft = React.useCallback(async () => {
    setSaveState("saving")
    await props.actions.onSaveGitCommitSettings({
      newBranchPrefix: draftPrefix,
      summaryModel: draftSummaryModel,
      summaryExtraPrompt: draftSummaryExtraPrompt,
      useRtkSummary,
    })
    setSaveState("saved")
    window.setTimeout(() => setSaveState("idle"), 1200)
  }, [draftPrefix, draftSummaryExtraPrompt, draftSummaryModel, props.actions, useRtkSummary])

  return (
    <section className="panel full-height settings-detail-panel">
      <PanelTitle icon={<GitCommitHorizontal size={17} />} title={text("Git 提交", "Git commit")} />
      <div className="settings-form-grid">
        <label className="settings-form-field">
          <span>{text("新分支默认前缀", "Default new-branch prefix")}</span>
          <input
            className="settings-input"
            onChange={(event) => setDraftPrefix(event.currentTarget.value)}
            placeholder={DEFAULT_COMMIT_BRANCH_PREFIX}
            value={draftPrefix}
          />
        </label>
        <article className="settings-card">
          <StatusPill status="ready" label={t("settings.gitCommit.active")} />
          <strong>{normalizedDraftPrefix}</strong>
          <p>{text("仅当前分支没有命名空间时使用；已有命名空间的分支会沿用当前命名空间。", "Used only when the current branch has no namespace; namespaced branches keep their current namespace.")}</p>
        </article>
        <article className="settings-card">
          <StatusPill status="ready" label={t("settings.gitCommit.autoSummary")} />
          <strong>{t("settings.gitCommit.summaryModel")}</strong>
          <div className="settings-card-actions">
            {draftSummaryModel.mode === "session" ? (
              <button className="skill-action-button" type="button">
                <Cpu size={14} />
                <span>{t("settings.gitCommit.followSessionModel")}</span>
              </button>
            ) : (
              <button className="skill-action-button" type="button">
                <Cpu size={14} />
                <span>{draftSummaryModel.providerId}/{draftSummaryModel.modelId}</span>
              </button>
            )}
            <ModelPickerControl
              ariaLabel={t("settings.gitCommit.chooseSummaryModel")}
              fallbackLabel={t("composer.selectFallback")}
              filterPlaceholder={t("composer.modelPicker.filter")}
              noMatchesLabel={t("composer.modelPicker.noMatches")}
              providers={props.workbench.providers}
              value={fixedSummaryModelValue}
              onValueChange={(choice) => setDraftSummaryModel({
                mode: "fixed",
                providerId: choice.providerId,
                modelId: choice.modelId,
              })}
            />
            <button
              className="skill-action-button"
              disabled={draftSummaryModel.mode === "session"}
              onClick={() => setDraftSummaryModel({ mode: "session" })}
              type="button"
            >
              <RotateCcw size={14} />
              <span>{t("settings.gitCommit.resetSummaryModel")}</span>
            </button>
          </div>
          <p>{draftSummaryModel.mode === "session" ? t("settings.gitCommit.followSessionModelHelp") : t("settings.gitCommit.fixedSummaryModelHelp")}</p>
          {draftSummaryModel.mode === "fixed" && !modelPickerChoiceExists(props.workbench.providers, fixedSummaryModelValue) ? (
            <p className="settings-muted-line">{t("settings.gitCommit.fixedSummaryModelUnavailable")}</p>
          ) : null}
        </article>
        <label className="settings-form-field">
          <span>{t("settings.gitCommit.summaryExtraPrompt")}</span>
          <textarea
            className="settings-input"
            onChange={(event) => setDraftSummaryExtraPrompt(event.currentTarget.value)}
            placeholder={t("settings.gitCommit.summaryExtraPromptPlaceholder")}
            rows={4}
            value={draftSummaryExtraPrompt}
          />
          <small className="settings-muted-line">{t("settings.gitCommit.summaryExtraPromptHelp")}</small>
        </label>
        <div className="settings-card-actions">
          <button className="skill-action-button" disabled={saveState === "saving"} onClick={() => void saveDraft()} type="button">
            {saveState === "saving" ? text("保存中", "Saving") : saveState === "saved" ? text("已保存", "Saved") : text("保存", "Save")}
          </button>
        </div>
      </div>
    </section>
  )
}

// normalizeGitCommitBranchPrefix mirrors the service-side prefix normalization for preview text.
// normalizeGitCommitBranchPrefix 对齐服务端前缀归一化逻辑以显示预览文本。
function normalizeGitCommitBranchPrefix(value: string): string {
  const trimmedValue = value.trim()
  if (!trimmedValue) {
    return DEFAULT_COMMIT_BRANCH_PREFIX
  }
  return trimmedValue.endsWith("/") ? trimmedValue : `${trimmedValue}/`
}

// modelPickerChoiceExists returns whether one provider/model key is currently selectable.
// modelPickerChoiceExists 返回某个 Provider/模型键当前是否可选。
function modelPickerChoiceExists(providers: WorkbenchSnapshot["providers"], value: string): boolean {
  if (!value) {
    return false
  }
  return providers.some((provider) => (provider.models ?? []).some((model) => modelPickerChoiceKey({
    providerId: provider.id,
    providerName: provider.name,
    modelId: model.id,
    modelName: model.name,
    active: Boolean(model.active),
  }) === value))
}

// RTK_GAIN_STATS_CACHE_MS keeps settings-page statistics fresh without rerunning RTK on every tab switch.
// RTK_GAIN_STATS_CACHE_MS 让设置页统计保持新鲜，同时避免每次切换标签都重复运行 RTK。
const RTK_GAIN_STATS_CACHE_MS = 30 * 60 * 1000

// RtkGainStatsCacheEntry stores one page-level RTK gain statistics cache entry.
// RtkGainStatsCacheEntry 存储单个页面级 RTK gain 统计缓存条目。
interface RtkGainStatsCacheEntry {
  // Timestamp in milliseconds when the cache entry was fetched.
  // 缓存条目拉取时的毫秒时间戳。
  loadedAtMs: number
  // Cached RTK gain statistics payload.
  // 缓存的 RTK gain 统计载荷。
  data: RtkGainStatsResponse | null
}

// RtkGainStatsView selects the period series shown by the chart.
// RtkGainStatsView 选择图表展示的周期序列。
type RtkGainStatsView = "daily" | "weekly" | "monthly"

// RtkGainChartPoint is a normalized data point for the compact SVG chart.
// RtkGainChartPoint 是紧凑 SVG 图表使用的归一化数据点。
interface RtkGainChartPoint {
  // Human-readable x-axis label.
  // 人类可读的横轴标签。
  label: string
  // Estimated tokens saved in this period.
  // 此周期内估算节省的 Token 数。
  savedTokens: number
  // Commands recorded in this period.
  // 此周期内记录的命令数。
  commands: number
  // Token-savings percentage for this period.
  // 此周期的 Token 节省百分比。
  savingsPct: number
}

// RtkGainChartCoordinate stores one plotted point with percent-based layout coordinates.
// RtkGainChartCoordinate 保存单个图表点及其基于百分比的布局坐标。
interface RtkGainChartCoordinate extends RtkGainChartPoint {
  // Horizontal position in chart percent units.
  // 图表百分比单位中的横向位置。
  x: number
  // Vertical position in chart percent units.
  // 图表百分比单位中的纵向位置。
  y: number
}

// rtkGainStatsCache stores the last RTK gain response for settings-page reuse.
// rtkGainStatsCache 存储设置页复用的最近一次 RTK gain 响应。
let rtkGainStatsCache: RtkGainStatsCacheEntry | null = null

// RtkRoutingSettingsArea renders RTK command-routing preferences.
// RtkRoutingSettingsArea 渲染 RTK 命令路由偏好。
export function RtkRoutingSettingsArea(props: {
  // Service-backed settings actions.
  // 服务端设置操作。
  actions: SettingsActionProps
  // Workbench snapshot that carries the shared host-tool configuration.
  // 携带共享宿主工具配置的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const rtkSettings = props.workbench.hostTools?.rtk ?? DEFAULT_RTK_SETTINGS
  const branchPrefix = props.workbench.gitCommit?.newBranchPrefix ?? DEFAULT_COMMIT_BRANCH_PREFIX
  const summaryModel = props.workbench.gitCommit?.summaryModel ?? DEFAULT_COMMIT_SUMMARY_MODEL
  const summaryExtraPrompt = props.workbench.gitCommit?.summaryExtraPrompt ?? ""
  const useRtkSummary = props.workbench.gitCommit?.useRtkSummary ?? true
  const summaryModelMode = summaryModel.mode
  const summaryModelProviderId = summaryModel.mode === "fixed" ? summaryModel.providerId : ""
  const summaryModelModelId = summaryModel.mode === "fixed" ? summaryModel.modelId : ""
  const [enabled, setEnabled] = React.useState(rtkSettings.enabled)
  const [routeMode, setRouteMode] = React.useState(rtkSettings.routeMode)
  const [enabledRouteSet, setEnabledRouteSet] = React.useState(() => rtkCheckedRouteSetFromSettings(rtkSettings))
  const [telemetryDisabled, setTelemetryDisabled] = React.useState(rtkSettings.telemetryDisabled)
  const [draftUseRtkSummary, setDraftUseRtkSummary] = React.useState(useRtkSummary)
  const [saveState, setSaveState] = React.useState<"idle" | "saving" | "saved">("idle")
  const [gainStats, setGainStats] = React.useState<RtkGainStatsResponse | null>(() => freshRtkGainStatsCache()?.data ?? null)
  const [gainLoadedAtMs, setGainLoadedAtMs] = React.useState<number | null>(() => freshRtkGainStatsCache()?.loadedAtMs ?? null)
  const [gainLoading, setGainLoading] = React.useState(false)
  const [gainView, setGainView] = React.useState<RtkGainStatsView>("daily")
  const mountedRef = React.useRef(true)
  const gainRequestIdRef = React.useRef(0)
  const getRtkGainStatsRef = React.useRef(props.actions.onGetRtkGainStats)

  React.useEffect(() => {
    setEnabled(rtkSettings.enabled)
    setRouteMode(rtkSettings.routeMode)
    setEnabledRouteSet(rtkCheckedRouteSetFromSettings(rtkSettings))
    setTelemetryDisabled(rtkSettings.telemetryDisabled)
  }, [rtkSettings.disabledRoutes, rtkSettings.enabled, rtkSettings.enabledRoutes, rtkSettings.routeMode, rtkSettings.telemetryDisabled])

  React.useEffect(() => {
    setDraftUseRtkSummary(useRtkSummary)
  }, [useRtkSummary])

  React.useEffect(() => {
    getRtkGainStatsRef.current = props.actions.onGetRtkGainStats
  }, [props.actions.onGetRtkGainStats])

  React.useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
    }
  }, [])

  const loadGainStats = React.useCallback(async (forceRefresh: boolean) => {
    const requestId = gainRequestIdRef.current + 1
    gainRequestIdRef.current = requestId
    const cachedStats = freshRtkGainStatsCache()
    if (!forceRefresh && cachedStats) {
      setGainStats(cachedStats.data)
      setGainLoadedAtMs(cachedStats.loadedAtMs)
      setGainLoading(false)
      return
    }

    setGainLoading(true)
    try {
      let nextStats: RtkGainStatsResponse | null
      try {
        nextStats = await getRtkGainStatsRef.current()
      } catch (error) {
        nextStats = unavailableRtkGainStatsResponse(Date.now(), error)
      }
      const loadedAtMs = Date.now()
      rtkGainStatsCache = { loadedAtMs, data: nextStats }
      if (mountedRef.current && gainRequestIdRef.current === requestId) {
        setGainStats(nextStats)
        setGainLoadedAtMs(loadedAtMs)
      }
    } finally {
      if (mountedRef.current && gainRequestIdRef.current === requestId) {
        setGainLoading(false)
      }
    }
  }, [])

  React.useEffect(() => {
    void loadGainStats(false)
  }, [loadGainStats])

  const enabledRoutes = React.useMemo(() => normalizedRouteSetValues(enabledRouteSet), [enabledRouteSet])
  const disabledRoutes = React.useMemo(() => {
    if (routeMode !== "all") {
      return []
    }
    return RTK_AVAILABLE_ROUTES.filter((route) => !enabledRouteSet.has(route))
  }, [enabledRouteSet, routeMode])
  const selectedRouteCount = enabledRoutes.length
  const gainSummary = gainStats?.summary ?? EMPTY_RTK_GAIN_SUMMARY
  const gainChartPoints = React.useMemo(() => rtkGainChartPoints(gainStats, gainView), [gainStats, gainView])
  const gainLoadedLabel = gainLoadedAtMs ? formatRtkGainTimestamp(gainLoadedAtMs, locale) : text("尚未统计", "Not loaded yet")
  const gainGeneratedLabel = gainStats?.generatedAtMs ? formatRtkGainTimestamp(gainStats.generatedAtMs, locale) : text("暂无", "None")
  const gainDensityRatio = gainSummary.totalOutput > 0 ? gainSummary.totalInput / gainSummary.totalOutput : 0
  const applyCuratedRoutes = React.useCallback(() => {
    setRouteMode("selected")
    setEnabledRouteSet(new Set(DEFAULT_RTK_ROUTES))
  }, [])
  const applyAllRoutes = React.useCallback(() => {
    setRouteMode("all")
    setEnabledRouteSet(new Set(RTK_AVAILABLE_ROUTES))
  }, [])
  const clearAllRoutes = React.useCallback(() => {
    setRouteMode("disabled")
    setEnabledRouteSet(new Set())
  }, [])
  const toggleRouteEnabled = React.useCallback((route: string, nextEnabled: boolean) => {
    setRouteMode((currentMode) => currentMode === "disabled" && nextEnabled ? "selected" : currentMode)
    setEnabledRouteSet((currentRoutes) => {
      const nextRoutes = new Set(currentRoutes)
      if (nextEnabled) {
        nextRoutes.add(route)
      } else {
        nextRoutes.delete(route)
      }
      return nextRoutes
    })
  }, [])
  const saveDraft = React.useCallback(async () => {
    setSaveState("saving")
    const saveRequests = [
      props.actions.onSaveRtkRoutingSettings({
        enabled,
        routeMode,
        enabledRoutes,
        disabledRoutes,
        telemetryDisabled,
      }),
    ]
    if (draftUseRtkSummary !== useRtkSummary) {
      saveRequests.push(props.actions.onSaveGitCommitSettings({
        newBranchPrefix: branchPrefix,
        summaryModel: summaryModelMode === "fixed"
          ? { mode: "fixed", providerId: summaryModelProviderId, modelId: summaryModelModelId }
          : { mode: "session" },
        summaryExtraPrompt,
        useRtkSummary: draftUseRtkSummary,
      }))
    }
    await Promise.all(saveRequests)
    setSaveState("saved")
    window.setTimeout(() => setSaveState("idle"), 1200)
  }, [branchPrefix, disabledRoutes, draftUseRtkSummary, enabled, enabledRoutes, props.actions, routeMode, summaryExtraPrompt, summaryModelMode, summaryModelModelId, summaryModelProviderId, telemetryDisabled, useRtkSummary])

  return (
    <section className="panel full-height settings-detail-panel">
      <PanelTitle icon={<Route size={17} />} title={text("RTK 命令路由", "RTK command routing")} />
      <div className="settings-form-grid">
        <article className="settings-card">
          <StatusPill status={enabled ? "ready" : "warning"} label={enabled ? text("已启用", "Enabled") : text("已关闭", "Disabled")} />
          <strong>{text("高密度命令上下文", "High-density command context")}</strong>
          <p>{text("RTK 会把测试、构建、搜索和 Git 输出中的重复日志压缩成更高密度的结果，让 AI 在同一轮上下文中保留更多有效分析内容，降低长日志挤占上下文导致的信息丢失。未匹配或失败时会自动回退原命令。", "RTK compresses noisy test, build, search, and Git output into denser results, so the AI can keep more useful analysis in the same context window and lose less information to long logs. Unmatched or failed rewrites fall back to the original command.")}</p>
          <label className="settings-checkbox-line">
            <input checked={enabled} onChange={(event) => setEnabled(event.currentTarget.checked)} type="checkbox" />
            <span>
              <strong>{text("启用 RTK 路由", "Enable RTK routing")}</strong>
              <small>{text("开启后，shell_command 会先尝试把支持的命令改写为 rtk 代理命令；关闭后所有命令按原样执行，不再压缩输出。", "When enabled, shell_command first tries to rewrite supported commands through the rtk proxy; when disabled, every command runs exactly as provided without output compression.")}</small>
            </span>
          </label>
          <label className="settings-checkbox-line">
            <input checked={draftUseRtkSummary} onChange={(event) => setDraftUseRtkSummary(event.currentTarget.checked)} type="checkbox" />
            <span>
              <strong>{text("提交生成使用 RTK", "Use RTK for commit generation")}</strong>
              <small>{text("开启后，自动生成提交信息会优先读取 RTK 压缩后的 git status 和 git diff 输出，让模型在同一上下文里看到更高密度的变更信息；随包 RTK 不存在或执行失败时会自动回退原始 diff。", "When enabled, automatic commit-message generation first reads RTK-compressed git status and git diff output, so the model sees denser change data in the same context. If bundled RTK is missing or fails, it falls back to the raw diff path.")}</small>
            </span>
          </label>
          <label className="settings-checkbox-line">
            <input checked={telemetryDisabled} onChange={(event) => setTelemetryDisabled(event.currentTarget.checked)} type="checkbox" />
            <span>
              <strong>{text("禁用 RTK 遥测环境变量", "Disable RTK telemetry environment variable")}</strong>
              <small>{text("开启后，Vulcan 执行 rtk rewrite 和 rtk 代理命令时会注入 RTK_TELEMETRY_DISABLED=1；关闭后遵循 RTK 自身默认遥测配置。", "When enabled, Vulcan injects RTK_TELEMETRY_DISABLED=1 for rtk rewrite and proxied commands; when disabled, RTK follows its own default telemetry configuration.")}</small>
            </span>
          </label>
        </article>
        <article className="settings-card rtk-gain-panel">
          <div className="rtk-gain-header">
            <div>
              <StatusPill
                status={gainStats?.available ? "ready" : "warning"}
                label={gainStats?.available ? text("统计可用", "Stats ready") : gainLoading ? text("读取中", "Loading") : text("未统计", "Unavailable")}
              />
              <strong>{text("RTK 上下文密度统计", "RTK context-density statistics")}</strong>
              <p>{text("这里展示 RTK 已经为命令输出节省了多少 Token，以及在同一上下文窗口里能承载多少更高密度的分析材料。页面打开时最多每 30 分钟自动刷新一次，切换日、周、月视图不会重复请求。", "This shows how many tokens RTK has saved for command output and how much denser analysis material can fit in the same context window. Opening the page refreshes at most once every 30 minutes, and switching daily, weekly, or monthly views does not call the backend again.")}</p>
            </div>
            <button className="skill-action-button secondary" disabled={gainLoading} onClick={() => void loadGainStats(true)} type="button">
              <RefreshCw className={gainLoading ? "spin" : undefined} size={14} />
              <span>{gainLoading ? text("刷新中", "Refreshing") : text("刷新", "Refresh")}</span>
            </button>
          </div>
          <div className="rtk-gain-meta">
            <span>{text(`上次统计：${gainLoadedLabel}`, `Last loaded: ${gainLoadedLabel}`)}</span>
            <span>{text(`数据生成：${gainGeneratedLabel}`, `Generated: ${gainGeneratedLabel}`)}</span>
          </div>
          {gainStats?.available ? (
            <>
              <div className="rtk-gain-kpi-grid">
                <div className="rtk-gain-kpi">
                  <span>{text("已节省 Token", "Tokens saved")}</span>
                  <strong>{formatCompactNumber(gainSummary.totalSaved, locale)}</strong>
                </div>
                <div className="rtk-gain-kpi">
                  <span>{text("平均节省", "Average savings")}</span>
                  <strong>{formatPercent(gainSummary.avgSavingsPct)}</strong>
                </div>
                <div className="rtk-gain-kpi">
                  <span>{text("记录命令", "Commands")}</span>
                  <strong>{formatCompactNumber(gainSummary.totalCommands, locale)}</strong>
                </div>
                <div className="rtk-gain-kpi">
                  <span>{text("承载倍率", "Capacity ratio")}</span>
                  <strong>{gainDensityRatio > 0 ? `${gainDensityRatio.toFixed(1)}x` : "0x"}</strong>
                </div>
              </div>
              <div className="rtk-gain-chart-toolbar">
                <div className="rtk-gain-tabs" role="tablist" aria-label={text("RTK 统计周期", "RTK statistics period")}>
                  {(["daily", "weekly", "monthly"] as const).map((view) => (
                    <button
                      aria-pressed={gainView === view}
                      className={gainView === view ? "rtk-gain-tab active" : "rtk-gain-tab"}
                      key={view}
                      onClick={() => setGainView(view)}
                      type="button"
                    >
                      {view === "daily" ? text("按天", "Day") : view === "weekly" ? text("按周", "Week") : text("按月", "Month")}
                    </button>
                  ))}
                </div>
                <small className="settings-muted-line">{text("折线展示周期节省 Token。", "The line shows tokens saved per period.")}</small>
              </div>
              <RtkGainLineChart
                commandsLabel={text("命令", "commands")}
                emptyLabel={text("当前周期暂无 RTK 统计。", "No RTK statistics for this period yet.")}
                locale={locale}
                points={gainChartPoints}
                savingsLabel={text("节省率", "savings")}
                savedLabel={text("已节省", "saved")}
              />
              <RtkGainCommandTable
                avgLabel={text("平均节省", "Avg savings")}
                commandLabel={text("命令", "Command")}
                countLabel={text("次数", "Count")}
                emptyLabel={text("暂无命令级 RTK 统计。", "No command-level RTK statistics yet.")}
                impactLabel={text("影响", "Impact")}
                locale={locale}
                rows={gainStats.byCommand ?? []}
                savedLabel={text("节省 Token", "Tokens saved")}
                timeLabel={text("耗时", "Time")}
                title={text("按命令统计", "By command")}
              />
            </>
          ) : (
            <div className="rtk-gain-empty">
              {gainLoading
                ? text("正在读取 RTK 统计...", "Loading RTK statistics...")
                : gainStats?.error
                  ? text(`读取失败：${gainStats.error}`, `Failed to load: ${gainStats.error}`)
                  : text("随包 RTK 可执行文件不可用，或当前还没有可展示的统计。", "The bundled RTK executable is unavailable, or there are no statistics to show yet.")}
            </div>
          )}
        </article>
        <div className="settings-card">
          <StatusPill
            status={routeMode === "disabled" ? "warning" : "ready"}
            label={routeMode === "all" ? text("全选", "All") : routeMode === "selected" ? text("精选", "Curated") : text("全关", "None")}
          />
          <strong>{text("路由表", "Route table")}</strong>
          <p>{text("勾选要让 RTK 接管的命令路由；全选后取消单个路由会保存为显式禁用。", "Check the command routes RTK may handle; clearing one route after selecting all is saved as an explicit deny.")}</p>
          <div className="settings-card-actions">
            <button className="skill-action-button secondary" onClick={applyCuratedRoutes} type="button">{text("只选精选", "Curated")}</button>
            <button className="skill-action-button secondary" onClick={applyAllRoutes} type="button">{text("全选", "Select all")}</button>
            <button className="skill-action-button secondary" onClick={clearAllRoutes} type="button">{text("全部取消", "Clear all")}</button>
          </div>
          <div className="settings-route-table-wrap">
            <table className="settings-route-table">
              <thead>
                <tr>
                  <th>{text("启用", "Enable")}</th>
                  <th>{text("路由", "Route")}</th>
                  <th>{text("类型", "Type")}</th>
                </tr>
              </thead>
              <tbody>
                {RTK_AVAILABLE_ROUTES.map((route) => {
                  const routeEnabled = enabledRouteSet.has(route)
                  return (
                    <tr
                      aria-checked={routeEnabled}
                      className="settings-route-row"
                      key={route}
                      onClick={() => toggleRouteEnabled(route, !routeEnabled)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter" || event.key === " ") {
                          event.preventDefault()
                          toggleRouteEnabled(route, !routeEnabled)
                        }
                      }}
                      role="checkbox"
                      tabIndex={0}
                    >
                      <td>
                        <input
                          aria-label={text(`启用 ${route} 路由`, `Enable ${route} route`)}
                          checked={routeEnabled}
                          onChange={(event) => toggleRouteEnabled(route, event.currentTarget.checked)}
                          onClick={(event) => event.stopPropagation()}
                          onKeyDown={(event) => event.stopPropagation()}
                          type="checkbox"
                        />
                      </td>
                      <td><strong>{route}</strong></td>
                      <td>{DEFAULT_RTK_ROUTE_SET.has(route) ? text("精选", "Curated") : text("扩展", "Extended")}</td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
          <small className="settings-muted-line">{text(`当前将应用 ${selectedRouteCount} 个 RTK 路由。`, `${selectedRouteCount} RTK routes will be applied.`)}</small>
        </div>
        <div className="settings-card-actions">
          <button className="skill-action-button" disabled={saveState === "saving"} onClick={() => void saveDraft()} type="button">
            <Save size={14} />
            <span>{saveState === "saving" ? text("保存中", "Saving") : saveState === "saved" ? text("已保存", "Saved") : text("保存", "Save")}</span>
          </button>
        </div>
      </div>
    </section>
  )
}

// DEFAULT_RTK_ROUTES mirrors the backend selected-route default for offline fallback snapshots.
// DEFAULT_RTK_ROUTES 对齐后端精选路由默认值，用于离线回退快照。
const DEFAULT_RTK_ROUTES = ["cargo", "find", "git", "go", "grep", "jest", "ls", "mypy", "npm", "npx", "pnpm", "pytest", "read", "ruff", "tree", "tsc", "vitest", "wc"]

// RTK_AVAILABLE_ROUTES mirrors RTK v0.42.4 rewrite route names after the `rtk` executable token.
// RTK_AVAILABLE_ROUTES 对齐 RTK v0.42.4 在 `rtk` 可执行文件 token 后暴露的 rewrite 路由名。
const RTK_AVAILABLE_ROUTES = [
  "ansible-playbook", "aws", "brew", "bundle", "cargo", "composer", "curl", "df", "diff", "docker",
  "dotnet", "du", "fail2ban-client", "find", "gcloud", "gh", "git", "glab", "go", "golangci-lint",
  "gradlew", "grep", "gt", "hadolint", "helm", "iptables", "jest", "kubectl", "lint", "liquibase",
  "ls", "make", "markdownlint", "mix", "mvn", "mypy", "next", "npm", "npx", "oc", "ping", "pio",
  "pip", "playwright", "pnpm", "poetry", "pre-commit", "prettier", "prisma", "ps", "psql", "pulumi",
  "pytest", "quarto", "rake", "read", "rg", "rspec", "rsync", "rubocop", "ruff", "shellcheck", "shopify",
  "sops", "swift", "systemctl", "terraform", "tofu", "tree", "trunk", "tsc", "uv", "vitest", "wc",
  "wget", "yamllint",
]

// DEFAULT_RTK_ROUTE_SET provides O(1) curated-route lookup for table labels.
// DEFAULT_RTK_ROUTE_SET 为表格标签提供 O(1) 精选路由查找。
const DEFAULT_RTK_ROUTE_SET = new Set(DEFAULT_RTK_ROUTES)

// DEFAULT_RTK_SETTINGS provides a stable fallback object when the service snapshot is older.
// DEFAULT_RTK_SETTINGS 在服务快照较旧时提供稳定的回退对象。
const DEFAULT_RTK_SETTINGS = {
  enabled: true,
  routeMode: "selected" as const,
  enabledRoutes: DEFAULT_RTK_ROUTES,
  disabledRoutes: [],
  telemetryDisabled: true,
}

// EMPTY_RTK_GAIN_SUMMARY provides stable numbers before statistics are loaded.
// EMPTY_RTK_GAIN_SUMMARY 在统计加载完成前提供稳定数值。
const EMPTY_RTK_GAIN_SUMMARY = {
  totalCommands: 0,
  totalInput: 0,
  totalOutput: 0,
  totalSaved: 0,
  avgSavingsPct: 0,
  totalTimeMs: 0,
  avgTimeMs: 0,
}

// rtkCheckedRouteSetFromSettings restores the route-table check state from persisted routing mode.
// rtkCheckedRouteSetFromSettings 根据持久化路由模式恢复路由表勾选状态。
function rtkCheckedRouteSetFromSettings(settings: {
  routeMode: "disabled" | "selected" | "all"
  enabledRoutes: string[]
  disabledRoutes: string[]
}): Set<string> {
  if (settings.routeMode === "disabled") {
    return new Set()
  }
  if (settings.routeMode === "all") {
    const disabledRouteSet = new Set(settings.disabledRoutes)
    return new Set(RTK_AVAILABLE_ROUTES.filter((route) => !disabledRouteSet.has(route)))
  }
  return new Set(settings.enabledRoutes)
}

// normalizedRouteSetValues converts a draft route set into a stable lowercase route list.
// normalizedRouteSetValues 将草稿路由集合转换为稳定的小写路由列表。
function normalizedRouteSetValues(value: Set<string>): string[] {
  return Array.from(new Set(Array.from(value).map((item) => item.trim().toLowerCase()).filter(Boolean))).sort()
}

// freshRtkGainStatsCache returns the cached statistics entry when it is still fresh.
// freshRtkGainStatsCache 在缓存仍新鲜时返回缓存统计条目。
function freshRtkGainStatsCache(): RtkGainStatsCacheEntry | null {
  if (!rtkGainStatsCache) {
    return null
  }
  return Date.now() - rtkGainStatsCache.loadedAtMs <= RTK_GAIN_STATS_CACHE_MS ? rtkGainStatsCache : null
}

// unavailableRtkGainStatsResponse converts a frontend load failure into a displayable stats payload.
// unavailableRtkGainStatsResponse 将前端加载失败转换为可展示的统计载荷。
function unavailableRtkGainStatsResponse(generatedAtMs: number, error: unknown): RtkGainStatsResponse {
  return {
    available: false,
    generatedAtMs,
    summary: EMPTY_RTK_GAIN_SUMMARY,
    daily: [],
    weekly: [],
    monthly: [],
    byCommand: [],
    error: error instanceof Error ? error.message : String(error),
  }
}

// rtkGainChartPoints normalizes RTK gain series for the compact chart.
// rtkGainChartPoints 将 RTK gain 序列归一化为紧凑图表数据。
function rtkGainChartPoints(stats: RtkGainStatsResponse | null, view: RtkGainStatsView): RtkGainChartPoint[] {
  if (!stats?.available) {
    return []
  }
  if (view === "daily") {
    return stats.daily.map((point) => rtkGainPeriodChartPoint(point, point.date.slice(5)))
  }
  if (view === "weekly") {
    return stats.weekly.map((point) => rtkGainPeriodChartPoint(point, `${point.weekStart.slice(5)}-${point.weekEnd.slice(5)}`))
  }
  return stats.monthly.map((point) => rtkGainPeriodChartPoint(point, point.month))
}

// rtkGainPeriodChartPoint converts one period point into chart display data.
// rtkGainPeriodChartPoint 将单个周期统计点转换为图表展示数据。
function rtkGainPeriodChartPoint(point: RtkGainPeriodStats, label: string): RtkGainChartPoint {
  return {
    label,
    savedTokens: point.savedTokens,
    commands: point.commands,
    savingsPct: point.savingsPct,
  }
}

// formatCompactNumber renders large metric values in a compact locale-aware form.
// formatCompactNumber 以本地化紧凑形式渲染大数指标。
function formatCompactNumber(value: number, locale: string): string {
  return new Intl.NumberFormat(locale, { notation: "compact", maximumFractionDigits: 1 }).format(value)
}

// formatPercent renders RTK savings percentage values.
// formatPercent 渲染 RTK 节省百分比。
function formatPercent(value: number): string {
  return `${value.toFixed(1)}%`
}

// formatDurationMs renders RTK duration values in compact milliseconds or seconds.
// formatDurationMs 将 RTK 耗时渲染为紧凑的毫秒或秒。
function formatDurationMs(value: number): string {
  if (value < 1000) {
    return `${Math.round(value)}ms`
  }
  if (value < 60_000) {
    return `${(value / 1000).toFixed(1)}s`
  }
  const minutes = Math.floor(value / 60_000)
  const seconds = Math.floor((value % 60_000) / 1000)
  return `${minutes}m${seconds}s`
}

// formatRtkGainTimestamp renders one statistics timestamp for the current locale.
// formatRtkGainTimestamp 按当前语言渲染统计时间戳。
function formatRtkGainTimestamp(value: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value))
}

// RtkGainLineChart renders a compact SVG line chart for RTK token savings.
// RtkGainLineChart 渲染 RTK Token 节省的紧凑 SVG 折线图。
function RtkGainLineChart(props: {
  // Normalized chart points.
  // 归一化后的图表数据点。
  points: RtkGainChartPoint[]
  // Locale used for compact number formatting.
  // 用于紧凑数字格式化的语言区域。
  locale: string
  // Label appended to the latest saved-token value.
  // 追加到最新节省 Token 数值后的标签。
  savedLabel: string
  // Label for command count in the point tooltip.
  // 点位提示中命令数量的标签。
  commandsLabel: string
  // Label for savings percentage in the point tooltip.
  // 点位提示中节省百分比的标签。
  savingsLabel: string
  // Empty-state label.
  // 空状态标签。
  emptyLabel: string
}) {
  if (props.points.length === 0) {
    return <div className="rtk-gain-empty">{props.emptyLabel}</div>
  }

  const maxSavedTokens = Math.max(...props.points.map((point) => point.savedTokens), 1)
  const coordinates: RtkGainChartCoordinate[] = props.points.map((point, index) => {
    const x = props.points.length === 1 ? 50 : (index / (props.points.length - 1)) * 100
    const y = props.points.length === 1 ? 48 : 88 - (point.savedTokens / maxSavedTokens) * 76
    return { ...point, x, y }
  })
  const linePath = coordinates.length === 1
    ? `M 8 ${coordinates[0].y.toFixed(2)} L 92 ${coordinates[0].y.toFixed(2)}`
    : coordinates.map((point, index) => `${index === 0 ? "M" : "L"} ${point.x.toFixed(2)} ${point.y.toFixed(2)}`).join(" ")
  const areaPath = coordinates.length === 1
    ? `M 8 ${coordinates[0].y.toFixed(2)} L 92 ${coordinates[0].y.toFixed(2)} L 92 96 L 8 96 Z`
    : `${linePath} L ${coordinates[coordinates.length - 1].x.toFixed(2)} 96 L ${coordinates[0].x.toFixed(2)} 96 Z`
  const lastPoint = coordinates[coordinates.length - 1]

  return (
    <div className="rtk-gain-chart">
      <div className="rtk-gain-chart-surface">
        <svg aria-hidden="true" preserveAspectRatio="none" viewBox="0 0 100 100">
          <defs>
            <linearGradient id="rtk-gain-area" x1="0" x2="0" y1="0" y2="1">
              <stop offset="0%" stopColor="var(--chart-2)" stopOpacity="0.32" />
              <stop offset="100%" stopColor="var(--chart-2)" stopOpacity="0.02" />
            </linearGradient>
          </defs>
          <line className="rtk-gain-gridline" x1="0" x2="100" y1="20" y2="20" />
          <line className="rtk-gain-gridline" x1="0" x2="100" y1="55" y2="55" />
          <line className="rtk-gain-gridline" x1="0" x2="100" y1="90" y2="90" />
          <path className="rtk-gain-area" d={areaPath} />
          <path className="rtk-gain-line" d={linePath} />
        </svg>
        {coordinates.map((point) => (
          <button
            aria-label={`${point.label}: ${formatCompactNumber(point.savedTokens, props.locale)} ${props.savedLabel}, ${point.commands} ${props.commandsLabel}, ${formatPercent(point.savingsPct)} ${props.savingsLabel}`}
            className="rtk-gain-point"
            key={`${point.label}-${point.savedTokens}-${point.commands}`}
            style={{ left: `${point.x}%`, top: `${point.y}%` }}
            type="button"
          >
            <span className="rtk-gain-point-dot" />
            <span className="rtk-gain-tooltip">
              <strong>{point.label}</strong>
              <span>{formatCompactNumber(point.savedTokens, props.locale)} {props.savedLabel}</span>
              <span>{point.commands} {props.commandsLabel}</span>
              <span>{formatPercent(point.savingsPct)} {props.savingsLabel}</span>
            </span>
          </button>
        ))}
      </div>
      <div className="rtk-gain-chart-footer">
        <span>{coordinates[0].label}</span>
        <strong>{formatCompactNumber(lastPoint.savedTokens, props.locale)} {props.savedLabel}</strong>
        <span>{lastPoint.label}</span>
      </div>
    </div>
  )
}

// RtkGainCommandTable renders command-level RTK gain details.
// RtkGainCommandTable 渲染 RTK gain 命令级统计明细。
function RtkGainCommandTable(props: {
  // Command-level statistics rows.
  // 命令级统计行。
  rows: NonNullable<RtkGainStatsResponse["byCommand"]>
  // Locale used for compact number formatting.
  // 用于紧凑数字格式化的语言区域。
  locale: string
  // Section title.
  // 区块标题。
  title: string
  // Command column label.
  // 命令列标签。
  commandLabel: string
  // Count column label.
  // 次数列标签。
  countLabel: string
  // Saved-token column label.
  // 节省 Token 列标签。
  savedLabel: string
  // Average savings column label.
  // 平均节省列标签。
  avgLabel: string
  // Time column label.
  // 耗时列标签。
  timeLabel: string
  // Impact column label.
  // 影响列标签。
  impactLabel: string
  // Empty-state label.
  // 空状态标签。
  emptyLabel: string
}) {
  const maxSavedTokens = Math.max(...props.rows.map((row) => row.savedTokens), 1)
  return (
    <section className="rtk-command-stats">
      <div className="rtk-command-stats-header">
        <strong>{props.title}</strong>
        <span>{props.rows.length > 0 ? `Top ${props.rows.length}` : props.emptyLabel}</span>
      </div>
      {props.rows.length > 0 ? (
        <div className="rtk-command-table-wrap">
          <table className="rtk-command-table">
            <thead>
              <tr>
                <th>#</th>
                <th>{props.commandLabel}</th>
                <th>{props.countLabel}</th>
                <th>{props.savedLabel}</th>
                <th>{props.avgLabel}</th>
                <th>{props.timeLabel}</th>
                <th>{props.impactLabel}</th>
              </tr>
            </thead>
            <tbody>
              {props.rows.map((row, index) => (
                <tr key={`${row.command}-${index}`}>
                  <td>{index + 1}</td>
                  <td><strong title={row.command}>{row.command}</strong></td>
                  <td>{formatCompactNumber(row.count, props.locale)}</td>
                  <td>{formatCompactNumber(row.savedTokens, props.locale)}</td>
                  <td>{formatPercent(row.avgSavingsPct)}</td>
                  <td>{formatDurationMs(row.avgTimeMs)}</td>
                  <td>
                    <span className="rtk-command-impact" aria-label={`${props.impactLabel}: ${formatPercent((row.savedTokens / maxSavedTokens) * 100)}`}>
                      <span style={{ width: `${Math.max(4, (row.savedTokens / maxSavedTokens) * 100)}%` }} />
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="rtk-gain-empty">{props.emptyLabel}</div>
      )}
    </section>
  )
}

// PromptComposerSettingsArea renders composer command and capability settings status.
// PromptComposerSettingsArea 渲染输入区指令与能力设置状态。
export function PromptComposerSettingsArea(props: {
  // Composer attachment capability gates.
  // 输入区附件能力门控。
  attachmentCapabilities: ComposerAttachmentCapabilitySummary[]
  // Backend-resolved reasoning selector view.
  // 后端解析出的推理选择器视图。
  reasoningSelection: WorkbenchSnapshot["reasoningSelection"] | null
  // Composer slash commands.
  // 输入区 slash 指令。
  slashCommands: ComposerSlashCommandSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  return (
    <div className="area-grid two-columns">
      <section className="panel settings-detail-panel">
        <PanelTitle icon={<FileText size={17} />} title={text("Slash 指令", "Slash commands")} />
        <div className="settings-target-list">
          {props.slashCommands.map((command) => (
            <article className="settings-target-row" key={command.id}>
              <div>
                <strong>{command.command}</strong>
                <span>{command.descriptionZh || command.descriptionEn}</span>
              </div>
              <Badge variant="outline">{command.id}</Badge>
            </article>
          ))}
        </div>
      </section>
      <section className="panel settings-detail-panel">
        <PanelTitle icon={<PackagePlus size={17} />} title={text("附件能力", "Attachment capability")} />
        <div className="settings-grid compact">
          {props.attachmentCapabilities.map((capability) => (
            <article className="settings-card" key={capability.kind}>
              <StatusPill status={capability.enabled ? "ready" : "warning"} label={capability.enabled ? "enabled" : "disabled"} />
              <strong>{capability.label}</strong>
              <p>{capability.native ? text("模型原生支持", "Native model support") : capability.bridged ? text("通过宿主工具桥接", "Bridged by host tools") : text("当前不可用", "Currently unavailable")}</p>
            </article>
          ))}
        </div>
      </section>
      <section className="panel settings-detail-panel">
        <PanelTitle icon={<SlidersHorizontal size={17} />} title={text("推理选择", "Reasoning selection")} />
        <div className="settings-target-list">
          {(props.reasoningSelection?.options ?? []).map((option) => (
            <article className="settings-target-row" key={option.value}>
              <div>
                <strong>{option.label}</strong>
                <span>{option.value}</span>
              </div>
              {option.value === props.reasoningSelection?.selectedEffort ? <Badge className="skill-card-enabled-badge">Selected</Badge> : <Badge variant="outline">Available</Badge>}
            </article>
          ))}
        </div>
      </section>
    </div>
  )
}

// AboutLicensesSettingsArea renders application version and license entry information.
// AboutLicensesSettingsArea 渲染应用版本与许可证入口信息。
export function AboutLicensesSettingsArea(props: {
  // Current application version.
  // 当前应用版本。
  appVersion: string
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  return (
    <section className="panel full-height settings-detail-panel">
      <PanelTitle icon={<ScrollText size={17} />} title={text("关于与许可证", "About and licenses")} />
      <div className="settings-grid">
        <article className="settings-card">
          <StatusPill status="ready" label="version" />
          <strong>Vulcan Code</strong>
          <p>{text(`当前版本：${props.appVersion}`, `Current version: ${props.appVersion}`)}</p>
        </article>
        <article className="settings-card">
          <StatusPill status="pending" label="licenses" />
          <strong>{text("许可证目录", "License directory")}</strong>
          <p>{text("WebUI 当前展示许可证入口状态，后续接入目录命令后可直接打开。", "The WebUI shows the license entry state until the directory command is connected.")}</p>
        </article>
      </div>
    </section>
  )
}

// ArchivedRestoreSettingsArea renders restorable archived conversations from service snapshots.
// ArchivedRestoreSettingsArea 根据服务快照渲染可恢复的归档会话。
export function ArchivedRestoreSettingsArea(props: {
  // Archived conversation summaries.
  // 归档会话摘要。
  conversations: ConversationSummary[]
  // Active project summaries.
  // 活动项目摘要。
  projects: ProjectSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const archivedConversations = props.conversations
  return (
    <section className="panel full-height settings-detail-panel">
      <PanelTitle icon={<Archive size={17} />} title={text("归档恢复", "Archived restore")} />
      <div className="settings-summary-strip">
        <StatusPill status={archivedConversations.length > 0 ? "warning" : "ready"} label={`${archivedConversations.length}`} />
        <span>{text(`当前 WebUI snapshot 暴露 ${props.projects.length} 个活动项目与 ${archivedConversations.length} 个可恢复归档会话。`, `The current WebUI snapshot exposes ${props.projects.length} active projects and ${archivedConversations.length} restorable archived conversations.`)}</span>
      </div>
      <div className="settings-target-list">
        {archivedConversations.length === 0 ? (
          <p className="settings-muted-line">{text("当前快照中没有可恢复的归档会话。", "No restorable archived conversations are exposed in the current snapshot.")}</p>
        ) : archivedConversations.map((conversation) => (
          <article className="settings-target-row" key={conversation.id}>
            <div>
              <strong>{conversation.title}</strong>
              <span>{conversation.projectId ?? "standalone"} · {conversation.lastUpdatedLabel ?? ""}</span>
            </div>
            <StatusPill status="warning" label={conversation.status} />
          </article>
        ))}
      </div>
    </section>
  )
}

// localizedLiteral picks the display literal that matches the active locale.
// localizedLiteral 按当前语言选择展示文本。
function localizedLiteral(locale: string, zh: string, en: string): string {
  return locale === "zh-CN" ? zh : en
}
