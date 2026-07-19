// DASHBOARD_CACHE_TTL_MS bounds module-level dashboard reuse across plugin page switches.
// DASHBOARD_CACHE_TTL_MS 限制插件页面切换时模块级仪表盘缓存的复用时间。
const DASHBOARD_CACHE_TTL_MS = 15 * 1000;

// dashboardCache preserves recent action results while the authenticated plugin module remains loaded.
// dashboardCache 在认证插件模块保持加载期间保留最近 Action 结果。
const dashboardCache = new Map();

// CHART_COLORS uses inherited Vulcan chart tokens so every active theme remains readable.
// CHART_COLORS 使用继承的 Vulcan 图表 Token，确保每个活动主题均可读。
const CHART_COLORS = ["var(--chart-1)", "var(--chart-2)", "var(--chart-3)", "var(--chart-4)", "var(--chart-5)", "var(--muted-foreground)"];

// RANGE_DAYS lists finite dashboard windows followed by zero for all retained history.
// RANGE_DAYS 依次列出有限仪表盘窗口，并以零表示全部保留历史。
const RANGE_DAYS = [7, 30];

// RETENTION_DAY_OPTIONS lists bounded raw-log periods followed by zero for permanent retention.
// RETENTION_DAY_OPTIONS 依次列出有界原始日志保留期，并以零表示永久保留。
const RETENTION_DAY_OPTIONS = [30, 90, 180, 365, 0];

// TOOLTIP_DELAY_MS keeps page chart details intentional instead of appearing on pointer transit.
// TOOLTIP_DELAY_MS 让页面图表详情仅在明确悬停后出现，避免指针经过时立即弹出。
const TOOLTIP_DELAY_MS = 500;

// Create the Provider usage statistics panel through the stable host UI SDK.
// 通过稳定宿主 UI SDK 创建 Provider 用量统计面板。
//
// sdk supplies host React, design-system components, localization, actions, and invalidation events.
// sdk 提供宿主 React、设计系统组件、本地化、Action 与失效事件。
// Returns one React component integrated into the plugin detail entry.
// 返回一个集成到插件详情入口的 React 组件。
export function createPluginPanel(sdk) {
  // React is the exact host renderer instance and must never be bundled by this plugin.
  // React 是精确宿主渲染器实例，当前插件绝不能自行打包。
  const React = sdk.React;
  // h is the host React element factory used by this uncompiled ESM module.
  // h 是当前未编译 ESM 模块使用的宿主 React 元素工厂。
  const h = React.createElement;
  // Badge is the shared Vulcan badge component.
  // Badge 是 Vulcan 共享徽标组件。
  const Badge = sdk.components.Badge;
  // Button is the shared Vulcan compact action component.
  // Button 是 Vulcan 共享紧凑操作按钮组件。
  const Button = sdk.components.Button;
  // StatusPill is the shared Vulcan lifecycle status component.
  // StatusPill 是 Vulcan 共享生命周期状态组件。
  const StatusPill = sdk.components.StatusPill;
  // MetricCard is the shared host KPI card.
  // MetricCard 是共享宿主 KPI 卡片。
  const MetricCard = sdk.components.MetricCard;
  // SelectControl is the shared host-themed combobox component.
  // SelectControl 是共享宿主主题下拉选择组件。
  const SelectControl = sdk.components.SelectControl;
  // ActivityHeatmap is the shared host calendar point grid.
  // ActivityHeatmap 是共享宿主日历点阵图。
  const ActivityHeatmap = sdk.components.ActivityHeatmap;
  // ActivityHeatmapLegend is the shared host five-level intensity legend.
  // ActivityHeatmapLegend 是共享宿主五级强度图例。
  const ActivityHeatmapLegend = sdk.components.ActivityHeatmapLegend;
  // TimeSeriesLineChart is the shared host SVG line chart.
  // TimeSeriesLineChart 是共享宿主 SVG 折线图。
  const TimeSeriesLineChart = sdk.components.TimeSeriesLineChart;
  // ChartLegend is the shared host interactive legend.
  // ChartLegend 是共享宿主交互图例。
  const ChartLegend = sdk.components.ChartLegend;
  // EmptyState is the shared host empty-state component.
  // EmptyState 是共享宿主空状态组件。
  const EmptyState = sdk.components.EmptyState;
  // ErrorState is the shared host error-state component.
  // ErrorState 是共享宿主错误状态组件。
  const ErrorState = sdk.components.ErrorState;
  // text resolves one plugin locale key with an explicit English source fallback.
  // text 使用明确英语源码回退解析一个插件语言键。
  const text = (key, fallback) => sdk.t(key, fallback);

  // interpolate replaces exact named locale placeholders without evaluating arbitrary templates.
  // interpolate 替换精确命名的语言占位符，不执行任意模板。
  function interpolate(value, fields) {
    // output accumulates deterministic literal replacements.
    // output 累积确定性的字面量替换。
    let output = value;
    for (const [key, fieldValue] of Object.entries(fields)) {
      output = output.replaceAll(`{{${key}}}`, String(fieldValue));
    }
    return output;
  }

  // numeric converts one Controller JSON scalar into a finite number.
  // numeric 将一个 Controller JSON 标量转换为有限数值。
  function numeric(value) {
    // number is the exact JavaScript numeric projection.
    // number 是精确的 JavaScript 数值投影。
    const number = Number(value);
    return Number.isFinite(number) ? number : 0;
  }

  // formatCompact renders one metric through the inherited host locale.
  // formatCompact 使用继承的宿主语言渲染一个紧凑指标。
  function formatCompact(value) {
    return new Intl.NumberFormat(sdk.locale, { notation: "compact", maximumFractionDigits: 1 }).format(numeric(value));
  }

  // formatPercent renders one percentage with one fractional digit.
  // formatPercent 以一位小数渲染一个百分比。
  function formatPercent(value) {
    return `${numeric(value).toFixed(1)}%`;
  }

  // formatDuration renders milliseconds using a compact stable unit.
  // formatDuration 使用紧凑稳定单位渲染毫秒数。
  function formatDuration(value) {
    // milliseconds is the normalized non-negative duration.
    // milliseconds 是规范化后的非负时长。
    const milliseconds = Math.max(0, numeric(value));
    if (milliseconds < 1000) return `${Math.round(milliseconds)} ms`;
    if (milliseconds < 60000) return `${(milliseconds / 1000).toFixed(1)} s`;
    return `${(milliseconds / 60000).toFixed(1)} min`;
  }

  // formatOutputTokenSpeed calculates visible output throughput from first token until completion.
  // formatOutputTokenSpeed 使用首 Token 到请求完成的真实区间计算可见输出吞吐率。
  //
  // request is one validated recent-request record containing output tokens and timing fields.
  // request 是一条包含输出 Token 与计时字段的已验证最近请求记录。
  // Returns a localized numeric t/s value, or an em dash when the generation interval is unavailable.
  // 返回本地化的数值 t/s；生成区间不可用时返回破折号。
  function formatOutputTokenSpeed(request) {
    if (request.timeToFirstTokenMs === null || request.timeToFirstTokenMs === undefined) return "—";
    // generationDurationMs excludes latency before the first visible token.
    // generationDurationMs 排除首个可见 Token 之前的等待时长。
    const generationDurationMs = numeric(request.durationMs) - numeric(request.timeToFirstTokenMs);
    if (generationDurationMs <= 0) return "—";
    // tokensPerSecond is the exact output-token throughput over the streaming interval.
    // tokensPerSecond 是流式输出区间内的精确输出 Token 吞吐率。
    const tokensPerSecond = numeric(request.outputTokens) / (generationDurationMs / 1000);
    return `${new Intl.NumberFormat(sdk.locale, { maximumFractionDigits: 1 }).format(tokensPerSecond)} t/s`;
  }

  // uncachedInputTokens reads the protocol-normalized non-cached input persisted by the statistics runtime.
  // uncachedInputTokens 读取统计运行时按协议归一化后持久化的非缓存输入。
  //
  // request is one validated recent-request record containing non-cached input and cache counts.
  // request 是一条包含非缓存输入量与缓存量的已验证最近请求记录。
  // Returns the non-negative non-cached input-token count.
  // 返回非负的非缓存输入 Token 数量。
  function uncachedInputTokens(request) {
    return Math.max(0, numeric(request.inputTokens));
  }

  // colorForSeries maps one stable model identity to the inherited chart palette.
  // colorForSeries 将一个稳定模型身份映射到继承的图表调色板。
  function colorForSeries(value) {
    // hash is a deterministic unsigned FNV-1a projection of the stable series key.
    // hash 是稳定序列键的确定性无符号 FNV-1a 投影。
    let hash = 2166136261;
    // index advances over UTF-16 code units without locale-sensitive normalization.
    // index 按 UTF-16 代码单元前进，不执行依赖语言环境的规范化。
    for (let index = 0; index < value.length; index += 1) {
      hash = Math.imul(hash ^ value.charCodeAt(index), 16777619);
    }
    return CHART_COLORS[(hash >>> 0) % CHART_COLORS.length];
  }

  // formatLocalDateKey converts one local Date into an ISO calendar key without UTC drift.
  // formatLocalDateKey 将一个本地 Date 转换为无 UTC 偏移的 ISO 自然日键。
  function formatLocalDateKey(date) {
    // year is the local calendar year.
    // year 是本地日历年。
    const year = date.getFullYear();
    // month is the zero-padded local calendar month.
    // month 是补零后的本地日历月。
    const month = String(date.getMonth() + 1).padStart(2, "0");
    // day is the zero-padded local calendar day.
    // day 是补零后的本地自然日。
    const day = String(date.getDate()).padStart(2, "0");
    return `${year}-${month}-${day}`;
  }

  // localCalendarDates returns an inclusive ordered local-date range ending today.
  // localCalendarDates 返回截至今天的有序本地自然日闭区间。
  function localCalendarDates(days, alignWeekStart) {
    // today is normalized to local midnight before date arithmetic.
    // today 在日期运算前规范化到本地午夜。
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    // start is the first requested local calendar date.
    // start 是请求的首个本地自然日。
    const start = new Date(today);
    start.setDate(start.getDate() - days + 1);
    if (alignWeekStart) {
      start.setDate(start.getDate() - start.getDay());
    }
    // dates accumulates every local date exactly once.
    // dates 恰好一次累积每个本地日期。
    const dates = [];
    for (const cursor = new Date(start); cursor <= today; cursor.setDate(cursor.getDate() + 1)) {
      dates.push(formatLocalDateKey(cursor));
    }
    return dates;
  }

  // lineChartDates resolves the selected finite window or the complete retained series span.
  // lineChartDates 解析选定有限窗口或完整保留序列跨度。
  function lineChartDates(rows, rangeDays) {
    if (rangeDays > 0) return localCalendarDates(rangeDays, false);
    if (rows.length === 0) return [];
    // firstDateKey is the earliest exact aggregate date returned by the all-time query.
    // firstDateKey 是累计查询返回的最早精确聚合日期。
    const firstDateKey = rows.reduce((earliest, row) => row.date < earliest ? row.date : earliest, rows[0].date);
    // start is the local midnight represented by the earliest aggregate date.
    // start 是最早聚合日期表示的本地午夜。
    const start = new Date(`${firstDateKey}T00:00:00`);
    // today bounds the complete retained history at the current local date.
    // today 把完整保留历史限制在当前本地日期。
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    // dates accumulates every retained local calendar key exactly once.
    // dates 恰好一次累积每个保留的本地自然日键。
    const dates = [];
    for (const cursor = new Date(start); cursor <= today; cursor.setDate(cursor.getDate() + 1)) {
      dates.push(formatLocalDateKey(cursor));
    }
    return dates;
  }

  // buildHeatmapCells fills exactly 365 dates and derives non-zero quantile intensities.
  // buildHeatmapCells 精确填补 365 个日期，并派生非零分位强度。
  function buildHeatmapCells(rows) {
    // rowsByDate maps exact ISO keys to complete daily activity facts.
    // rowsByDate 将精确 ISO 键映射到完整每日活动事实。
    const rowsByDate = new Map(rows.map((row) => [row.date, row]));
    // nonZeroValues is the ordered population used for five quantile bands.
    // nonZeroValues 是用于五个分位区间的有序非零总体。
    const nonZeroValues = rows
      .map((row) => numeric(row.value))
      .filter((value) => value > 0)
      .sort((left, right) => left - right);
    // thresholds contains the inclusive 20%, 40%, 60%, 80%, and 100% cutoffs.
    // thresholds 包含闭区间的 20%、40%、60%、80% 与 100% 阈值。
    const thresholds = nonZeroValues.length === 0
      ? []
      : [1, 2, 3, 4, 5].map((level) => nonZeroValues[Math.ceil((nonZeroValues.length * level) / 5) - 1]);
    return localCalendarDates(365, false).map((date) => {
      // row is the complete aggregate for this date when activity exists.
      // row 是存在活动时当前日期的完整聚合。
      const row = rowsByDate.get(date);
      // value is the exact daily total or zero for a missing aggregate row.
      // value 是精确每日总量；缺失聚合行时为零。
      const value = numeric(row?.value);
      // intensity is the first inclusive quantile band containing the non-zero value.
      // intensity 是包含该非零值的首个闭区间分位等级。
      const thresholdIndex = value === 0 ? -1 : thresholds.findIndex((threshold) => value <= threshold);
      const intensity = value === 0 ? 0 : thresholdIndex < 0 ? 5 : thresholdIndex + 1;
      // requests is the exact physical request count for the tooltip.
      // requests 是提示中使用的精确真实请求数。
      const requests = numeric(row?.requests);
      // successes is the exact successful request count for the tooltip.
      // successes 是提示中使用的精确成功请求数。
      const successes = numeric(row?.successes);
      // successRate avoids dividing an empty day while preserving exact aggregate semantics.
      // successRate 在保留精确聚合语义的同时避免空日期除零。
      const successRate = requests === 0 ? 0 : successes * 100 / requests;
      // topModel is the permanent last-known provider/model display pair.
      // topModel 是永久保留的最后已知供应商/模型显示组合。
      const topModel = row?.topModelName
        ? `${row.topProviderName} / ${row.topModelName}`
        : text("heatmap.noModel", "No model");
      return {
        date,
        intensity,
        label: interpolate(
          text("heatmap.tooltip", "{{date}}: {{value}} tokens · {{requests}} requests · {{successRate}} success · {{model}}"),
          {
            date: formatHeatmapDate(date),
            model: topModel,
            requests: sdk.formatNumber(requests),
            successRate: formatPercent(successRate),
            value: sdk.formatNumber(value),
          },
        ),
      };
    });
  }

  // formatMonthLabel renders one ISO calendar date as a locale-aware short month.
  // formatMonthLabel 将一个 ISO 自然日渲染为本地化短月份名称。
  function formatMonthLabel(date) {
    return new Intl.DateTimeFormat(sdk.locale, { month: "short" }).format(new Date(`${date}T00:00:00`));
  }

  // formatHeatmapDate renders one exact local calendar date for a compact tooltip.
  // formatHeatmapDate 为紧凑提示渲染一个精确的本地自然日。
  //
  // date is the ISO local-calendar key emitted by the statistics action.
  // date 是统计 Action 输出的 ISO 本地自然日键。
  // Returns a locale-aware month-and-day label without changing the represented date.
  // 返回本地化的月日标签，且不改变其表示的日期。
  function formatHeatmapDate(date) {
    return new Intl.DateTimeFormat(sdk.locale, { month: "long", day: "numeric" }).format(new Date(`${date}T00:00:00`));
  }

  // buildLineSeries aligns sparse daily model rows onto one shared local-date axis.
  // buildLineSeries 将稀疏每日模型行对齐到共享本地日期轴。
  function buildLineSeries(rows, rangeDays) {
    // dates is the complete selected local-calendar x-axis.
    // dates 是完整的已选本地自然日横轴。
    const dates = lineChartDates(rows, rangeDays);
    // groups accumulates exact values by stable series key and date.
    // groups 按稳定序列键与日期累积精确数值。
    const groups = new Map();
    for (const row of rows) {
      if (!groups.has(row.seriesKey)) {
        groups.set(row.seriesKey, { label: row.label, values: new Map() });
      }
      groups.get(row.seriesKey).values.set(row.date, numeric(row.value));
    }
    return Array.from(groups.entries()).map(([id, group]) => ({
      id,
      label: id === "other" ? text("other", "Other") : group.label,
      color: colorForSeries(id),
      points: dates.map((date) => ({ key: date, value: group.values.get(date) ?? 0 })),
    }));
  }

  // dashboardCacheKey identifies one exact query projection.
  // dashboardCacheKey 标识一个精确查询投影。
  //
  // rangeDays, metric, ownerKind, and modelRef are the complete active filter tuple.
  // rangeDays、metric、ownerKind 与 modelRef 是完整的活动筛选元组。
  // Returns one collision-free cache key for the selected projection.
  // 返回选定投影使用的无冲突缓存键。
  function dashboardCacheKey(rangeDays, metric, ownerKind, modelRef) {
    return `${rangeDays}|${metric}|${ownerKind ?? "all"}|${modelRef ?? "all"}`;
  }

  // UsageStatisticsPanel renders settings, KPIs, heatmap, trend, models, and recent requests.
  // UsageStatisticsPanel 渲染设置、KPI、热力图、趋势、模型与最近请求。
  function UsageStatisticsPanel() {
    // settings stores the last host-validated plugin configuration.
    // settings 保存最近一次经宿主校验的插件配置。
    const [settings, setSettings] = React.useState(null);
    // rangeDays selects the aggregate window.
    // rangeDays 选择聚合窗口。
    const [rangeDays, setRangeDays] = React.useState(30);
    // metric selects the line-chart measurement.
    // metric 选择折线图度量。
    const [metric, setMetric] = React.useState("total_tokens");
    // ownerKind selects one exact execution owner category or all categories.
    // ownerKind 选择一个精确执行归属类别或全部类别。
    const [ownerKind, setOwnerKind] = React.useState(null);
    // modelRef selects one exact persistent Provider/model identity or every model.
    // modelRef 选择一个精确的持久 Provider/模型身份或全部模型。
    const [modelRef, setModelRef] = React.useState(null);
    // dashboard stores the latest aggregate action result.
    // dashboard 保存最近的聚合 Action 结果。
    const [dashboard, setDashboard] = React.useState(null);
    // recent stores bounded recent raw facts.
    // recent 保存有界的最近原始事实。
    const [recent, setRecent] = React.useState([]);
    // errors stores bounded redacted failure categories for the active range.
    // errors 保存活动时间范围内有界的脱敏失败类别。
    const [errors, setErrors] = React.useState([]);
    // loading indicates one active dashboard refresh.
    // loading 表示一次活动仪表盘刷新。
    const [loading, setLoading] = React.useState(true);
    // saving indicates one active configuration write.
    // saving 表示一次活动配置写入。
    const [saving, setSaving] = React.useState(false);
    // error stores the latest bounded action diagnostic.
    // error 保存最近的有界 Action 诊断。
    const [error, setError] = React.useState("");
    // hiddenSeries stores user-hidden line identities.
    // hiddenSeries 保存用户隐藏的折线身份。
    const [hiddenSeries, setHiddenSeries] = React.useState(() => new Set());
    // summaryExpanded controls whether KPI cards beyond the default two rows are visible.
    // summaryExpanded 控制默认两行以外的 KPI 卡片是否可见。
    const [summaryExpanded, setSummaryExpanded] = React.useState(false);
    // revisionRef lets event callbacks compare against the latest rendered revision.
    // revisionRef 使事件回调可以比较最近渲染的修订号。
    const revisionRef = React.useRef(0);
    // refreshTimerRef coalesces rapid completion events into one dashboard reload.
    // refreshTimerRef 将快速连续的完成事件合并为一次仪表盘重载。
    const refreshTimerRef = React.useRef(null);

    // loadDashboard reads one cached or current dashboard projection plus recent facts.
    // loadDashboard 读取一个缓存或当前仪表盘投影及最近事实。
    const loadDashboard = React.useCallback(async (force) => {
      // cacheKey identifies the exact active query projection.
      // cacheKey 标识精确的活动查询投影。
      const cacheKey = dashboardCacheKey(rangeDays, metric, ownerKind, modelRef);
      // cached is the optional fresh module-level action result.
      // cached 是可选的新鲜模块级 Action 结果。
      const cached = dashboardCache.get(cacheKey);
      if (!force && cached && Date.now() - cached.loadedAtMs < DASHBOARD_CACHE_TTL_MS) {
        setDashboard(cached.dashboard);
        setRecent(cached.recent);
        setErrors(cached.errors);
        revisionRef.current = numeric(cached.dashboard.revision);
        setLoading(false);
        return;
      }
      setLoading(true);
      setError("");
      try {
        // ownerArguments omits the optional field for the all-owner view instead of sending JSON null.
        // ownerArguments 在全部归属视图中省略可选字段，而不是发送 JSON null。
        const ownerArguments = ownerKind === null ? {} : { ownerKind };
        // modelArguments omits the optional field for the all-model view instead of sending JSON null.
        // modelArguments 在全部模型视图中省略可选字段，而不是发送 JSON null。
        const modelArguments = modelRef === null ? {} : { modelRef };
        // results are loaded concurrently because both reads are independent and bounded.
        // results 并发加载，因为两次读取相互独立且有界。
        const [dashboardResult, recentResult, errorsResult] = await Promise.all([
          sdk.invoke("dashboard-get", { rangeDays, metric, ...ownerArguments, ...modelArguments }),
          sdk.invoke("recent-get", { limit: 20, ...ownerArguments, ...modelArguments }),
          sdk.invoke("errors-get", { rangeDays, metric, ...ownerArguments, ...modelArguments }),
        ]);
        // requests is the exact recent facts array from the validated action output.
        // requests 是经校验 Action 输出中的精确最近事实数组。
        const requests = recentResult.requests;
        // groupedErrors is the validated bounded error-category projection.
        // groupedErrors 是经校验的有界错误类别投影。
        const groupedErrors = errorsResult.errors;
        dashboardCache.set(cacheKey, { dashboard: dashboardResult, recent: requests, errors: groupedErrors, loadedAtMs: Date.now() });
        setDashboard(dashboardResult);
        setRecent(requests);
        setErrors(groupedErrors);
        revisionRef.current = numeric(dashboardResult.revision);
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        setLoading(false);
      }
    }, [metric, modelRef, ownerKind, rangeDays]);

    React.useEffect(() => {
      // cancelled prevents a stale settings read from mutating an unmounted panel.
      // cancelled 防止旧设置读取修改已卸载面板。
      let cancelled = false;
      void sdk.invoke("settings-get", {}).then((value) => {
        if (!cancelled) {
          setSettings(value);
        }
      }).catch((reason) => {
        if (!cancelled) setError(reason instanceof Error ? reason.message : String(reason));
      });
      return () => { cancelled = true; };
    }, []);

    React.useEffect(() => {
      void loadDashboard(false);
    }, [loadDashboard]);

    React.useEffect(() => {
      // scheduleRefresh debounces a burst of request-completion revisions.
      // scheduleRefresh 对一批请求完成修订进行防抖。
      const scheduleRefresh = (revision) => {
        if (revision <= revisionRef.current || refreshTimerRef.current !== null) return;
        refreshTimerRef.current = globalThis.setTimeout(() => {
          refreshTimerRef.current = null;
          void loadDashboard(true);
        }, 250);
      };
      // unsubscribe releases the plugin-bound revision listener synchronously.
      // unsubscribe 同步释放插件绑定的修订监听器。
      const unsubscribe = sdk.events.subscribe("statistics.changed", (revision) => {
        scheduleRefresh(revision);
      });
      return () => {
        unsubscribe();
        if (refreshTimerRef.current !== null) {
          globalThis.clearTimeout(refreshTimerRef.current);
          refreshTimerRef.current = null;
        }
      };
    }, [loadDashboard]);

    React.useEffect(() => {
      // timer provides a visible-page fallback when backend event transport is unavailable.
      // timer 在后端事件传输不可用时提供可见页面轮询回退。
      const timer = globalThis.setInterval(() => {
        if (globalThis.document?.visibilityState !== "visible") return;
        void sdk.invoke("revision-get", {}).then((value) => {
          if (numeric(value.revision) > revisionRef.current) void loadDashboard(true);
        }).catch(() => {
          // Event transport errors remain visible through explicit refreshes; polling stays silent.
          // 事件传输错误仍可通过显式刷新发现；轮询保持静默。
        });
      }, 10000);
      return () => globalThis.clearInterval(timer);
    }, [loadDashboard]);

    // updateSettings persists one complete configuration immediately after a direct user choice.
    // updateSettings 在用户直接选择后立即持久化一份完整配置。
    //
    // nextSettings is the optimistic complete configuration and successMessage is the localized confirmation.
    // nextSettings 是乐观更新后的完整配置，successMessage 是本地化成功提示。
    // Returns a promise that resolves after the host accepts the update or the previous value is restored.
    // 返回在宿主接受更新或恢复旧值后完成的 Promise。
    async function updateSettings(nextSettings, successMessage) {
      if (settings === null || saving) return;
      // previousSettings is restored exactly if host validation or persistence fails.
      // previousSettings 在宿主校验或持久化失败时被精确恢复。
      const previousSettings = settings;
      // normalizedSettings pins removed defaults without exposing redundant controls.
      // normalizedSettings 在不暴露冗余控件的情况下固定已移除的默认项。
      const normalizedSettings = { ...nextSettings, defaultRangeDays: 30, defaultMetric: "total_tokens" };
      setSettings(normalizedSettings);
      setSaving(true);
      setError("");
      try {
        // updated is the exact host-persisted configuration.
        // updated 是宿主精确持久化后的配置。
        const updated = await sdk.invoke("settings-update", normalizedSettings);
        setSettings(updated);
        sdk.notifications.success(successMessage);
      } catch (reason) {
        // message is the bounded host diagnostic shown both inline and through the system Toast.
        // message 是同时以内联方式和系统 Toast 展示的有界宿主诊断。
        const message = reason instanceof Error ? reason.message : String(reason);
        setSettings(previousSettings);
        setError(message);
        sdk.notifications.error(message);
      } finally {
        setSaving(false);
      }
    }

    // toggleSeries changes only local chart visibility and never reloads statistics.
    // toggleSeries 仅改变本地图表可见性，绝不重新加载统计。
    function toggleSeries(id) {
      setHiddenSeries((current) => {
        // next is an immutable visibility-set update.
        // next 是不可变的可见性集合更新。
        const next = new Set(current);
        if (next.has(id)) next.delete(id); else next.add(id);
        return next;
      });
    }

    // summary is the normalized KPI object or an empty placeholder before first load.
    // summary 是规范化 KPI 对象；首次加载前为空占位对象。
    const summary = dashboard?.summary ?? {};
    // heatmapCells contains the complete aligned calendar point grid.
    // heatmapCells 包含完整对齐的日历点阵。
    const heatmapCells = buildHeatmapCells(dashboard?.heatmap ?? []);
    // allSeries contains complete aligned visible and hidden model lines.
    // allSeries 包含完整对齐的可见与隐藏模型折线。
    const allSeries = buildLineSeries(dashboard?.series ?? [], rangeDays);
    // visibleSeries excludes locally hidden lines without changing the action result.
    // visibleSeries 排除本地隐藏折线，不修改 Action 结果。
    const visibleSeries = allSeries.filter((series) => !hiddenSeries.has(series.id));
    // legendItems projects chart lines into stable visibility controls.
    // legendItems 将图表折线投影为稳定的可见性控件。
    const legendItems = allSeries.map((series) => ({ ...series, visible: !hiddenSeries.has(series.id) }));
    // metricFormatter selects the exact compact formatter for the active metric.
    // metricFormatter 为活动指标选择精确紧凑格式化器。
    const metricFormatter = metric === "duration_ms" || metric === "time_to_first_token_ms" ? formatDuration : formatCompact;
    // summaryCards contains all sixteen user-visible KPIs in semantic reading order.
    // summaryCards 按语义阅读顺序包含全部十六张用户可见 KPI 卡片。
    const summaryCards = [
      h(MetricCard, { key: "tokens", icon: "tokens", label: text("summary.tokens", "Token usage"), value: formatCompact(summary.totalTokens) }),
      h(MetricCard, { key: "input-tokens", icon: "tokens", label: text("summary.inputTokens", "Input tokens"), value: formatCompact(summary.inputTokens) }),
      h(MetricCard, { key: "output-tokens", icon: "tokens", label: text("summary.outputTokens", "Output tokens"), value: formatCompact(summary.outputTokens) }),
      h(MetricCard, { key: "requests", icon: "requests", label: text("summary.requests", "Requests"), value: formatCompact(summary.requests), detail: interpolate(text("summary.retries", "{{count}} retries"), { count: formatCompact(summary.retries) }) }),
      h(MetricCard, { key: "success-rate", icon: "success", label: text("summary.successRate", "Success rate"), value: formatPercent(summary.successRate), detail: `${formatCompact(summary.failures)} ${text("status.failed", "failed")} · ${formatCompact(summary.cancelled)} ${text("status.cancelled", "cancelled")}` }),
      h(MetricCard, { key: "first-token", icon: "duration", label: text("summary.firstToken", "Average first token"), value: formatDuration(summary.averageFirstTokenMs) }),
      h(MetricCard, { key: "average-duration", icon: "duration", label: text("summary.averageDuration", "Average duration"), value: formatDuration(summary.averageDurationMs) }),
      h(MetricCard, { key: "active-days", icon: "calendar", label: text("summary.activeDays", "Active days"), value: sdk.formatNumber(numeric(summary.activeDays)) }),
      h(MetricCard, { key: "streak", icon: "calendar", label: text("summary.streak", "Current streak"), value: `${sdk.formatNumber(numeric(summary.currentStreakDays))} ${text("unit.days", "days")}`, detail: interpolate(text("summary.longestStreak", "Longest streak {{days}} days"), { days: sdk.formatNumber(numeric(summary.longestStreakDays)) }) }),
      h(MetricCard, { key: "top-model", icon: "model", label: text("summary.topModel", "Most used model"), value: dashboard?.models?.[0]?.modelName ?? "—", detail: dashboard?.models?.[0] ? `${formatPercent(dashboard.models[0].sharePercent)} · ${dashboard.models[0].providerName}` : "" }),
      h(MetricCard, { key: "cached-tokens", icon: "tokens", label: text("summary.cachedTokens", "Cached input tokens"), value: formatCompact(summary.cachedInputTokens), detail: `${formatPercent(summary.cacheHitRate)} ${text("summary.cacheRate", "cache hit rate")}` }),
      h(MetricCard, { key: "peak-tokens", icon: "activity", label: text("summary.peakTokens", "Peak daily tokens"), value: formatCompact(summary.peakDailyTokens), detail: interpolate(text("summary.peakRequest", "{{count}} per request"), { count: formatCompact(summary.peakRequestTokens) }) }),
      h(MetricCard, { key: "longest-request", icon: "duration", label: text("summary.longestRequest", "Longest request"), value: formatDuration(summary.longestRequestMs) }),
      h(MetricCard, { key: "main-agent", icon: "agent", label: text("summary.mainAgentTokens", "Main agent tokens"), value: formatCompact(summary.mainAgentTokens) }),
      h(MetricCard, { key: "subagent", icon: "subagent", label: text("summary.subagentTokens", "Subagent tokens"), value: formatCompact(summary.subagentTokens) }),
      h(MetricCard, { key: "internal", icon: "internal", label: text("summary.internalTokens", "Internal task tokens"), value: formatCompact(summary.internalTokens) }),
    ];
    // primarySummaryCards always exposes exactly two four-card rows.
    // primarySummaryCards 始终显示两行、每行四张卡片。
    const primarySummaryCards = summaryCards.slice(0, 8);
    // additionalSummaryCards stays mounted so CSS can animate its measured content height in both directions.
    // additionalSummaryCards 保持挂载，使 CSS 能按实际内容高度双向执行展开与收起动画。
    const additionalSummaryCards = summaryCards.slice(8);

    return h("section", { className: "full-height settings-detail-panel plugin-statistics-layout" },
      h("div", { className: "plugin-statistics-header" },
        h("div", null,
          h("strong", null, text("title", "API Usage Statistics")),
          h("p", null, text("subtitle", "Measures physical Provider requests by model and owner without storing prompts, outputs, headers, or credentials.")),
        ),
        h(StatusPill, { status: error ? "warning" : "ready", label: loading ? text("status.loading", "Loading statistics") : error ? text("status.error", "Statistics failed to load") : text("status.ready", "Statistics available") }),
      ),
      settings ? h("div", { className: "plugin-statistics-settings-list" },
        h("label", { className: "settings-checkbox-line plugin-statistics-settings-option" },
            h("input", {
              checked: settings.enabled,
              disabled: saving,
              onChange: (event) => {
                // enabled is captured synchronously before React releases the input event.
                // enabled 在 React 释放输入事件前同步捕获。
                const enabled = event.currentTarget.checked;
                void updateSettings(
                  { ...settings, enabled },
                  enabled ? text("settings.enabledSuccess", "Statistics enabled") : text("settings.disabledSuccess", "Statistics disabled"),
                );
              },
              type: "checkbox",
            }),
            h("span", null, h("strong", null, text("settings.enabled", "Enable statistics"))),
          ),
        h("div", { className: "plugin-statistics-settings-option" },
          h("strong", null, text("settings.retention", "Log retention")),
            h(SelectControl, {
              ariaLabel: text("settings.retention", "Log retention"),
              className: "plugin-statistics-retention-select",
              disabled: saving,
              value: String(settings.retentionDays),
              options: RETENTION_DAY_OPTIONS.map((days) => ({
                value: String(days),
                label: days === 0 ? text("settings.retentionPermanent", "Permanent") : interpolate(text("settings.retentionDays", "{{days}} days"), { days }),
              })),
              onValueChange: (value) => void updateSettings(
                { ...settings, retentionDays: Number(value) },
                text("settings.retentionSuccess", "Log retention updated"),
              ),
            }),
        ),
      ) : null,
      h("div", { className: "plugin-statistics-range-header" },
        h("div", { className: "plugin-statistics-toolbar" },
          h(Button, {
            "aria-busy": loading,
            "aria-label": loading ? text("refreshing", "Refreshing") : text("refresh", "Refresh"),
            className: "plugin-statistics-refresh-button",
            disabled: loading,
            onClick: () => void loadDashboard(true),
            size: "sm",
            title: loading ? text("refreshing", "Refreshing") : text("refresh", "Refresh"),
            type: "button",
            variant: "outline",
          }, h("svg", {
            "aria-hidden": "true",
            className: loading ? "plugin-statistics-refresh-icon is-loading" : "plugin-statistics-refresh-icon",
            fill: "none",
            viewBox: "0 0 24 24",
          }, h("path", {
            d: "M20 12a8 8 0 1 1-2.34-5.66L20 8.68M20 4v4.68h-4.68",
            stroke: "currentColor",
            strokeLinecap: "round",
            strokeLinejoin: "round",
            strokeWidth: "1.75",
          }))),
          h(SelectControl, {
            ariaLabel: text("metric.label", "Trend metric"),
            className: "plugin-statistics-toolbar-select",
            value: metric,
            options: ["total_tokens", "input_tokens", "output_tokens", "requests", "duration_ms", "time_to_first_token_ms"].map((value) => ({ value, label: text(`metric.${value}`, value) })),
            onValueChange: setMetric,
          }),
          h(SelectControl, {
            ariaLabel: text("model.label", "Model"),
            className: "plugin-statistics-toolbar-select",
            value: modelRef === null ? "all" : String(modelRef),
            options: [
              { value: "all", label: text("model.all", "All models") },
              ...(dashboard?.availableModels ?? []).map((model) => ({
                value: String(model.modelRef),
                label: `${model.modelName} · ${model.providerName}`,
              })),
            ],
            onValueChange: (value) => {
              // nextModelRef preserves the exact persistent model identity selected by the user.
              // nextModelRef 保留用户选择的精确持久模型身份。
              const nextModelRef = value === "all" ? null : Number(value);
              setModelRef(nextModelRef);
              // A newly focused model must not inherit a hidden line from a previous chart view.
              // 新聚焦的模型不得继承上一图表视图中的隐藏折线状态。
              setHiddenSeries(new Set());
            },
          }),
          h(SelectControl, {
            ariaLabel: text("owner.label", "Request owner"),
            className: "plugin-statistics-toolbar-select",
            value: ownerKind ?? "all",
            options: ["all", "main_agent", "subagent", "context_compaction", "git_commit_summary"].map((value) => ({ value, label: text(`owner.${value}`, value) })),
            onValueChange: (value) => setOwnerKind(value === "all" ? null : value),
          }),
        ),
        h("div", { className: "plugin-statistics-range-control" },
          h("span", null, text("range.label", "Time range")),
          h("div", { className: "plugin-statistics-range-tabs", role: "tablist", "aria-label": text("range.label", "Time range") },
            ...RANGE_DAYS.map((days) => h(Button, {
              key: days,
              size: "xs",
              variant: rangeDays === days ? "secondary" : "ghost",
              role: "tab",
              "aria-selected": rangeDays === days,
              onClick: () => setRangeDays(days),
              type: "button",
            }, text(`range.${days}`, days === 0 ? "All time" : `Last ${days} days`))),
          ),
        ),
      ),
      error ? h(ErrorState, { title: text("status.error", "Statistics failed to load"), description: error }) : null,
      h("div", { className: "plugin-statistics-summary-section" },
        h("div", { className: "plugin-statistics-summary-grid" },
          ...primarySummaryCards,
        ),
        h("div", {
          "aria-hidden": !summaryExpanded,
          className: summaryExpanded ? "plugin-statistics-summary-extra is-expanded" : "plugin-statistics-summary-extra",
        },
          h("div", { className: "plugin-statistics-summary-extra-clip" },
            h("div", { className: "plugin-statistics-summary-grid" }, ...additionalSummaryCards),
          ),
        ),
        h("div", { className: "plugin-statistics-summary-actions" },
          h(Button, {
            "aria-expanded": summaryExpanded,
            "aria-label": summaryExpanded ? text("summary.collapse", "Show less") : text("summary.expand", "Show more"),
            className: "plugin-statistics-summary-toggle",
            onClick: () => setSummaryExpanded((current) => !current),
            size: "xs",
            title: summaryExpanded ? text("summary.collapse", "Show less") : text("summary.expand", "Show more"),
            type: "button",
            variant: "outline",
          }, h("svg", {
            "aria-hidden": "true",
            className: "plugin-statistics-summary-toggle-icon",
            fill: "none",
            viewBox: "0 0 24 24",
          }, h("path", {
            d: summaryExpanded ? "m18 15-6-6-6 6" : "m6 9 6 6 6-6",
            stroke: "currentColor",
            strokeLinecap: "round",
            strokeLinejoin: "round",
            strokeWidth: "1.75",
          }))),
        ),
      ),
      h("article", { className: "plugin-statistics-card" },
        h("div", { className: "plugin-statistics-section-header" }, h("strong", null, text("heatmap.title", "Activity heatmap")), h(ActivityHeatmapLegend, { lowLabel: text("heatmap.less", "Less"), highLabel: text("heatmap.more", "More") })),
        h("div", { className: "plugin-statistics-chart-wrap" }, h(ActivityHeatmap, { cells: heatmapCells, ariaLabel: text("heatmap.aria", "Token activity heatmap for the last 365 days"), emptyLabel: text("heatmap.empty", "No token activity yet"), formatMonth: formatMonthLabel, tooltipDelayMs: TOOLTIP_DELAY_MS })),
      ),
      h("article", { className: "plugin-statistics-card" },
        h("div", { className: "plugin-statistics-section-header" }, h("strong", null, `${text("trend.title", "Daily trend")} · ${text(`metric.${metric}`, metric)}`), h(Badge, { variant: "outline" }, text(`range.${rangeDays}`, `${rangeDays} days`))),
        h("div", { className: "plugin-statistics-chart-wrap" }, h(TimeSeriesLineChart, { series: visibleSeries, ariaLabel: text("trend.aria", "Daily statistics line chart grouped by model"), formatValue: metricFormatter, emptyLabel: text("trend.empty", "No trend data in this range"), tooltipDelayMs: TOOLTIP_DELAY_MS })),
        h(ChartLegend, { items: legendItems, onToggle: toggleSeries }),
      ),
      h("article", { className: "plugin-statistics-card" },
        h("div", { className: "plugin-statistics-section-header" }, h("strong", null, text("models.title", "Model usage")), h(Badge, { variant: "outline" }, interpolate(text("models.top", "Top {{count}}"), { count: Math.min(10, dashboard?.models?.length ?? 0) }))),
        dashboard?.models?.length ? h("div", { className: "plugin-statistics-model-list" }, ...dashboard.models.map((model) => h("div", { className: "plugin-statistics-model-row", key: model.modelRef },
          h("span", { className: "plugin-statistics-model-name", title: `${model.providerName} / ${model.modelName}` }, h("strong", null, model.modelName), " · ", model.providerName),
          h("span", null, interpolate(text("models.tokens", "{{count}} tokens"), { count: formatCompact(model.totalTokens) })),
          h("span", null, `${formatPercent(model.sharePercent)} · ${interpolate(text("models.requests", "{{count}} requests"), { count: formatCompact(model.requests) })}`),
        ))) : h(EmptyState, { title: text("models.empty", "No model usage in this range") }),
      ),
      h("article", { className: "plugin-statistics-card" },
        h("div", { className: "plugin-statistics-section-header" }, h("strong", null, text("errors.title", "Failure categories")), h(Badge, { variant: "outline" }, Math.min(20, errors.length))),
        errors.length ? h("div", { className: "plugin-statistics-model-list" }, ...errors.map((item) => h("div", { className: "plugin-statistics-model-row", key: `${item.errorCategory}:${item.errorCode}` },
          h("span", { className: "plugin-statistics-model-name" }, h("strong", null, item.errorCategory), item.errorCode ? ` · ${item.errorCode}` : ""),
          h("span", null, `${formatCompact(item.count)} ${text("errors.count", "failures")}`),
        ))) : h(EmptyState, { title: text("errors.empty", "No failures in this range") }),
      ),
      h("article", { className: "plugin-statistics-card" },
        h("div", { className: "plugin-statistics-section-header" }, h("strong", null, text("recent.title", "Recent requests")), h(Badge, { variant: "outline" }, Math.min(20, recent.length))),
        recent.length ? h("div", { className: "plugin-statistics-table-wrap" }, h("table", { className: "plugin-statistics-table" },
          h("thead", null, h("tr", null, ...["time", "model", "owner", "status", "tokenBreakdown", "speed", "firstToken", "duration"].map((key) => h("th", { key }, text(`recent.${key}`, key))))),
          h("tbody", null, ...recent.map((request) => h("tr", { key: request.requestId },
            h("td", null, new Intl.DateTimeFormat(sdk.locale, { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" }).format(new Date(numeric(request.completedAtMs)))),
            h("td", { title: `${request.providerName} / ${request.modelName}` }, request.modelName),
            h("td", null, text(`owner.${request.ownerKind}`, request.ownerName)),
            h("td", { className: "plugin-statistics-status", "data-status": request.status }, text(`status.${request.status}`, request.status)),
            h("td", { className: "plugin-statistics-token-breakdown" }, `${formatCompact(uncachedInputTokens(request))} / ${formatCompact(request.outputTokens)} / ${formatCompact(request.cachedInputTokens)}`),
            h("td", null, formatOutputTokenSpeed(request)),
            h("td", null, request.timeToFirstTokenMs === null || request.timeToFirstTokenMs === undefined ? "—" : formatDuration(request.timeToFirstTokenMs)),
            h("td", null, formatDuration(request.durationMs)),
          ))),
        )) : h(EmptyState, { title: text("recent.empty", "No raw request records yet") }),
      ),
      h("p", { className: "plugin-statistics-privacy" }, text("privacy", "Raw requests retain statistics fields only. Model dimensions and daily aggregates persist even after a model is removed from configuration.")),
    );
  }

  return UsageStatisticsPanel;
}
