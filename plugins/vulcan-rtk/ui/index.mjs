// RTK statistics page. Routing switches and the route lists live in the declarative settings mount (ui/settings.json);
// this module renders token-saving statistics through the host page kit.
// RTK 统计页面。路由开关与路由清单位于声明式设置挂载（ui/settings.json）；
// 本模块通过宿主页面套件渲染 Token 节省统计。

// RTK_REPOSITORY_URL is the upstream project address disclosed for license compliance.
// RTK_REPOSITORY_URL 是为许可证合规披露的上游项目地址。
const RTK_REPOSITORY_URL = "https://github.com/rtk-ai/rtk";

// RTK_LICENSE_PATH is the exact manifest-declared package license asset.
// RTK_LICENSE_PATH 是清单明确声明的包内许可证资源路径。
const RTK_LICENSE_PATH = "LICENSE";

// PERIOD_VIEWS lists the chart series RTK reports; switching never issues another request.
// PERIOD_VIEWS 列出 RTK 报告的图表序列；切换不会发起新请求。
const PERIOD_VIEWS = ["daily", "weekly", "monthly"];

// CHART_POINT_LIMIT keeps the most recent periods readable on narrow pages.
// CHART_POINT_LIMIT 让窄页面上的最近周期保持可读。
const CHART_POINT_LIMIT = 14;

// createPluginPanel binds the host SDK and returns the statistics page component.
// createPluginPanel 绑定宿主 SDK 并返回统计页面组件。
export function createPluginPanel(sdk) {
  const React = sdk.React;
  const h = React.createElement;
  const {
    Button, MetricCard, MetricGrid, PageBody, PageEmptyState, PageHint, PageNotice, PageSection, PageTable, PageToolbar,
    PluginMetadataLink, StatusPill, StreamPlaceholder, TextDocumentDialog, TimeSeriesLineChart,
  } = sdk.components;
  const text = (key, fallback) => sdk.t(key, fallback);

  // numberField reads one RTK metric, accepting only its documented snake-case and host-normalized camel-case names.
  // numberField 读取一个 RTK 指标，仅接受其文档化蛇形名称和宿主规范化驼峰名称。
  // Returns a finite numeric value or zero.
  // 返回有限数值，否则返回零。
  function numberField(record, ...names) {
    for (const name of names) {
      const value = Number(record?.[name]);
      if (Number.isFinite(value)) return value;
    }
    return 0;
  }

  function formatCompactNumber(value) {
    return new Intl.NumberFormat(sdk.locale, { notation: "compact", maximumFractionDigits: 1 }).format(value);
  }

  function formatPercent(value) {
    return new Intl.NumberFormat(sdk.locale, { style: "percent", minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(value / 100);
  }

  // formatDurationMs renders RTK command durations in compact milliseconds, seconds or minutes.
  // formatDurationMs 将 RTK 命令耗时渲染为紧凑的毫秒、秒或分钟文本。
  function formatDurationMs(value) {
    if (value < 1000) return new Intl.NumberFormat(sdk.locale, { style: "unit", unit: "millisecond", unitDisplay: "short", maximumFractionDigits: 0 }).format(value);
    if (value < 60000) return new Intl.NumberFormat(sdk.locale, { style: "unit", unit: "second", unitDisplay: "short", minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(value / 1000);
    return `${new Intl.NumberFormat(sdk.locale, { style: "unit", unit: "minute", unitDisplay: "short" }).format(Math.floor(value / 60000))} ${new Intl.NumberFormat(sdk.locale, { style: "unit", unit: "second", unitDisplay: "short" }).format(Math.floor((value % 60000) / 1000))}`;
  }

  // formatTimestamp renders one Unix millisecond timestamp, or the plugin-owned empty label.
  // formatTimestamp 渲染一个 Unix 毫秒时间戳，或插件自有的空值文案。
  function formatTimestamp(value) {
    if (!Number.isFinite(value) || value <= 0) return text("stats.none", "None");
    return new Intl.DateTimeFormat(sdk.locale, { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" }).format(new Date(value));
  }

  // formatPeriodDate formats RTK's ISO calendar dates in UTC so the day never shifts across time zones.
  // value is YYYY-MM-DD for daily and weekly data or YYYY-MM for monthly data; undocumented shapes stay visible as-is.
  // formatPeriodDate 以 UTC 格式化 RTK 的 ISO 日历日期，避免跨时区改变日期。
  // value 为日、周数据的 YYYY-MM-DD 或月数据的 YYYY-MM；未声明的格式原样显示。
  function formatPeriodDate(value, monthOnly = false) {
    if (!(monthOnly ? /^\d{4}-\d{2}$/ : /^\d{4}-\d{2}-\d{2}$/).test(value)) return value;
    const date = new Date(`${value}${monthOnly ? "-01" : ""}T00:00:00Z`);
    if (!Number.isFinite(date.getTime()) || date.toISOString().slice(0, monthOnly ? 7 : 10) !== value) return value;
    return new Intl.DateTimeFormat(sdk.locale, monthOnly
      ? { year: "numeric", month: "short", timeZone: "UTC" }
      : { month: "numeric", day: "numeric", timeZone: "UTC" }).format(date);
  }

  // periodPoints converts one RTK series into chart points keyed by their localized period label.
  // periodPoints 将一个 RTK 序列转换为以本地化周期标签为键的图表点。
  function periodPoints(statistics, view) {
    const source = Array.isArray(statistics?.[view]) ? statistics[view] : [];
    return source.slice(-CHART_POINT_LIMIT).map((point) => ({
      key: view === "daily"
        ? formatPeriodDate(String(point.date ?? ""))
        : view === "weekly"
          ? `${formatPeriodDate(String(point.week_start ?? point.weekStart ?? ""))} – ${formatPeriodDate(String(point.week_end ?? point.weekEnd ?? ""))}`
          : formatPeriodDate(String(point.month ?? ""), true),
      value: numberField(point, "saved_tokens", "savedTokens"),
    }));
  }

  function RtkStatisticsPage() {
    const [statistics, setStatistics] = React.useState(null);
    const [loadedAtMs, setLoadedAtMs] = React.useState(null);
    const [view, setView] = React.useState("daily");
    const [loading, setLoading] = React.useState(true);
    const [error, setError] = React.useState("");
    const [licenseOpen, setLicenseOpen] = React.useState(false);
    const [licenseContent, setLicenseContent] = React.useState("");
    const [licenseLoading, setLicenseLoading] = React.useState(false);
    const [licenseError, setLicenseError] = React.useState("");
    // mounted and sequence reject completions from an unmounted page or a superseded read.
    // mounted 与 sequence 拒绝已卸载页面或被取代读取的完成结果。
    const mounted = React.useRef(false);
    const sequence = React.useRef(0);

    // load reads statistics through the host cache declared by the action; force bypasses only that cache.
    // load 通过 Action 声明的宿主缓存读取统计；force 只绕过该缓存。
    const load = React.useCallback(async (force) => {
      const current = ++sequence.current;
      setLoading(true);
      setError("");
      try {
        const result = await sdk.invoke("statistics-get", {}, { refresh: force });
        if (!mounted.current || current !== sequence.current) return;
        setStatistics(result);
        setLoadedAtMs(Date.now());
      } catch (reason) {
        if (mounted.current && current === sequence.current) setError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        if (mounted.current && current === sequence.current) setLoading(false);
      }
    }, []);

    React.useEffect(() => {
      mounted.current = true;
      void load(false);
      return () => { mounted.current = false; sequence.current += 1; };
    }, [load]);

    async function openRepository() {
      try {
        await sdk.openExternalUrl(RTK_REPOSITORY_URL);
      } catch (reason) {
        sdk.notifications.error(`${text("compliance.githubOpenFailed", "Failed to open GitHub repository")}: ${reason instanceof Error ? reason.message : String(reason)}`);
      }
    }

    // openLicense opens the dialog immediately and reads the packaged LICENSE once.
    // openLicense 立即打开对话框，并只读取一次包内 LICENSE。
    async function openLicense() {
      setLicenseOpen(true);
      if (licenseContent || licenseLoading) return;
      setLicenseLoading(true);
      setLicenseError("");
      try {
        const content = await sdk.readTextAsset(RTK_LICENSE_PATH);
        if (mounted.current) setLicenseContent(content);
      } catch (reason) {
        if (mounted.current) setLicenseError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        if (mounted.current) setLicenseLoading(false);
      }
    }

    const available = statistics?.available === true;
    const summary = statistics?.summary ?? {};
    const byCommand = Array.isArray(statistics?.byCommand) ? statistics.byCommand : [];
    const ratio = numberField(summary, "total_input", "totalInput") / Math.max(1, numberField(summary, "total_output", "totalOutput"));

    let overview;
    if (statistics === null && loading) {
      overview = h(StreamPlaceholder, { count: 4, label: text("stats.loading", "Loading"), shape: "tiles" });
    } else if (available) {
      overview = h(React.Fragment, null,
        h(MetricGrid, null,
          h(MetricCard, { icon: "tokens", label: text("stats.savedTokens", "Tokens saved"), value: formatCompactNumber(numberField(summary, "total_saved", "totalSaved")) }),
          h(MetricCard, { icon: "success", label: text("stats.averageSavings", "Average savings"), value: formatPercent(numberField(summary, "avg_savings_pct", "avgSavingsPct")) }),
          h(MetricCard, { icon: "requests", label: text("stats.commands", "Commands"), value: formatCompactNumber(numberField(summary, "total_commands", "totalCommands")) }),
          h(MetricCard, { icon: "activity", label: text("stats.capacityRatio", "Context capacity"),
            value: `${new Intl.NumberFormat(sdk.locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(ratio)}×` }),
        ),
        h(PageToolbar, {
          actions: PERIOD_VIEWS.map((value) => h(Button, {
            "aria-pressed": view === value, key: value, onClick: () => setView(value), size: "sm", type: "button", variant: view === value ? "secondary" : "ghost",
          }, text(`stats.${value}`, value))),
        }, h(PageHint, null, text("stats.chartLabel", "RTK token savings trend"))),
        h(TimeSeriesLineChart, {
          ariaLabel: text("stats.chartLabel", "RTK token savings trend"),
          emptyLabel: text("stats.empty", "No statistics yet"),
          formatValue: formatCompactNumber,
          series: [{ id: "saved", label: text("stats.savedTokens", "Tokens saved"), color: "var(--primary)", points: periodPoints(statistics, view) }],
        }),
      );
    } else if (statistics !== null) {
      overview = h(PageEmptyState, { description: statistics.error ?? undefined, title: text("stats.unavailableHelp", "Statistics are currently unavailable.") });
    } else {
      overview = null;
    }

    return h(PageBody, null,
      h(PageToolbar, {
        actions: [
          h(PluginMetadataLink, { ariaLabel: text("compliance.githubAria", "Open the RTK GitHub repository"), href: RTK_REPOSITORY_URL, icon: "github", key: "github",
            label: "rtk-ai/rtk", onActivate: () => void openRepository() }),
          h(PluginMetadataLink, { ariaLabel: text("compliance.licenseAria", "Open the Apache-2.0 license"), icon: "license", key: "license",
            label: "Apache-2.0", onActivate: () => void openLicense() }),
        ],
      }, h(PageHint, null,
        `${text("stats.lastLoaded", "Last loaded")}: ${formatTimestamp(loadedAtMs)} · ${text("stats.generated", "Generated")}: ${formatTimestamp(numberField(statistics, "generated_at_ms", "generatedAtMs"))}`)),
      error ? h(PageNotice, {
        actions: h(Button, { disabled: loading, onClick: () => void load(true), size: "sm", type: "button", variant: "outline" }, text("retry", "Retry")),
        details: error, detailsLabel: text("errorDetails", "Error details"), tone: "error",
      }, text("stats.loadFailed", "Unable to load RTK statistics")) : null,
      h(PageSection, {
        actions: h(Button, { disabled: loading, onClick: () => void load(true), size: "sm", type: "button", variant: "outline" },
          loading ? text("stats.refreshing", "Refreshing") : text("stats.refresh", "Refresh")),
        description: text("stats.description", "Token savings are cached for thirty minutes. Switching daily, weekly, and monthly views never sends another request."),
        status: statistics === null ? null : h(StatusPill, {
          label: available ? text("stats.ready", "Stats ready") : text("stats.unavailable", "Unavailable"), status: available ? "ready" : "warning" }),
        title: text("stats.title", "RTK context-density statistics"),
      }, overview),
      available ? h(PageSection, {
        status: byCommand.length > 0 ? h(PageHint, null, text("stats.topCommands", "Top {{count}} commands").replace("{{count}}", sdk.formatNumber(byCommand.length))) : null,
        title: text("stats.byCommand", "By command"),
      }, h(PageTable, {
        columns: [
          { key: "rank", label: "#", numeric: true },
          { key: "command", label: text("stats.command", "Command") },
          { key: "count", label: text("stats.count", "Count"), numeric: true },
          { key: "saved", label: text("stats.saved", "Tokens saved"), numeric: true },
          { key: "savings", label: text("stats.savingsRate", "Savings"), numeric: true },
          { key: "time", label: text("stats.time", "Time"), numeric: true },
        ],
        empty: h(PageEmptyState, { title: text("stats.noCommandBreakdown", "No command breakdown yet") }),
        label: text("stats.byCommand", "By command"),
        rows: byCommand.map((row, index) => ({
          key: `${row.command ?? "command"}:${index}`,
          cells: {
            rank: sdk.formatNumber(index + 1),
            command: h("code", null, row.command ?? "-"),
            count: sdk.formatNumber(numberField(row, "count", "commands")),
            saved: sdk.formatNumber(numberField(row, "saved_tokens", "savedTokens")),
            savings: formatPercent(numberField(row, "avg_savings_pct", "avgSavingsPct", "savings_pct", "savingsPct")),
            time: formatDurationMs(numberField(row, "avg_time_ms", "avgTimeMs")),
          },
        })),
      })) : null,
      h(TextDocumentDialog, {
        content: licenseContent,
        description: text("compliance.licenseDescription", "License text read directly from the installed RTK plugin package."),
        error: licenseError,
        loading: licenseLoading,
        loadingLabel: text("compliance.licenseLoading", "Loading license"),
        onOpenChange: setLicenseOpen,
        open: licenseOpen,
        title: text("compliance.licenseTitle", "RTK Apache-2.0 License"),
      }),
    );
  }

  return RtkStatisticsPage;
}
