// Context7 usage page. Authentication and request behavior live in the declarative settings mount (ui/settings.json);
// this module only reads local statistics and the latest quota snapshot through the host page kit.
// Context7 用量页面。认证与请求行为位于声明式设置挂载（ui/settings.json）；
// 本模块只通过宿主页面套件读取本地统计与最新配额快照。

// CONTEXT7_DASHBOARD_URL is the fixed official account and API-key management entry.
// CONTEXT7_DASHBOARD_URL 是固定的官方账户与 API Key 管理入口。
const CONTEXT7_DASHBOARD_URL = "https://context7.com/dashboard";

// CONTEXT7_USAGE_DOCS_URL is the fixed official usage explanation page.
// CONTEXT7_USAGE_DOCS_URL 是固定的官方用量说明页面。
const CONTEXT7_USAGE_DOCS_URL = "https://context7.com/docs/howto/usage";

// RANGE_OPTIONS lists every bounded local statistics window accepted by the usage-get input schema.
// RANGE_OPTIONS 列出 usage-get 输入 Schema 接受的全部有界本地统计时间窗。
const RANGE_OPTIONS = [7, 30, 90, 365];

// DEFAULT_RANGE_DAYS is the window shown when the page opens.
// DEFAULT_RANGE_DAYS 是页面打开时显示的时间窗。
const DEFAULT_RANGE_DAYS = 30;

// DATA_TOPIC is the invalidation topic the Lua runtime publishes after recording a request or credential change.
// DATA_TOPIC 是 Lua 运行时在记录请求或凭据变更后发布的失效主题。
const DATA_TOPIC = "context7.data";

// createPluginPanel binds the host SDK and returns the usage page component.
// createPluginPanel 绑定宿主 SDK 并返回用量页面组件。
export function createPluginPanel(sdk) {
  const React = sdk.React;
  const h = React.createElement;
  const {
    Badge, Button, MetricCard, MetricGrid, PageBody, PageEmptyState, PageNotice, PageSection, SelectControl, StreamPlaceholder,
  } = sdk.components;
  const text = (key, fallback) => sdk.t(key, fallback);

  // formatUnit renders one value with a locale-aware unit.
  // formatUnit 以语言相关单位渲染一个值。
  function formatUnit(value, unit) {
    return new Intl.NumberFormat(sdk.locale, { style: "unit", unit, unitDisplay: unit === "day" ? "long" : "short" }).format(value);
  }

  // formatQuotaResetDate renders the provider reset time as a locale date without a time of day.
  // formatQuotaResetDate 将服务商重置时间渲染为不含时刻的本地日期。
  function formatQuotaResetDate(unixSeconds) {
    return new Intl.DateTimeFormat(sdk.locale, { day: "numeric", month: "short", year: "numeric" }).format(new Date(unixSeconds * 1000));
  }

  // count renders an optional non-negative integer, using an em dash when absent.
  // count 渲染可选的非负整数，缺失时显示破折号。
  function count(value) {
    return typeof value === "number" ? sdk.formatNumber(value) : "—";
  }

  function Context7UsagePage() {
    const [rangeDays, setRangeDays] = React.useState(DEFAULT_RANGE_DAYS);
    const [usage, setUsage] = React.useState(null);
    const [loading, setLoading] = React.useState(true);
    const [error, setError] = React.useState(null);
    // sequence discards responses of superseded reads.
    // sequence 丢弃被取代读取的响应。
    const sequence = React.useRef(0);

    const load = React.useCallback(async (selectedRangeDays, options) => {
      const current = ++sequence.current;
      setLoading(true);
      setError(null);
      try {
        const result = await sdk.invoke("usage-get", { rangeDays: selectedRangeDays }, options);
        if (current === sequence.current) setUsage(result);
      } catch (loadError) {
        if (current === sequence.current) setError(loadError instanceof Error ? loadError.message : String(loadError));
      } finally {
        if (current === sequence.current) setLoading(false);
      }
    }, []);

    React.useEffect(() => {
      void load(rangeDays);
      return () => { sequence.current += 1; };
    }, [load, rangeDays]);

    React.useEffect(() => sdk.events.subscribe(DATA_TOPIC, () => void load(rangeDays)), [load, rangeDays]);

    // openOfficial opens one fixed official page and reports host refusals as a toast.
    // openOfficial 打开一个固定官方页面，并以通知报告宿主拒绝。
    async function openOfficial(url) {
      try {
        await sdk.openExternalUrl(url);
      } catch (openError) {
        sdk.notifications.error(openError instanceof Error ? openError.message : String(openError));
      }
    }

    function officialLink(url, label) {
      return h(Button, { key: url, onClick: () => void openOfficial(url), size: "sm", type: "button", variant: "link" }, label);
    }

    if (loading && usage === null && error === null) {
      return h(PageBody, null, h(StreamPlaceholder, { label: text("usageTitle", "Usage and quota"), shape: "cards" }));
    }

    const stats = usage?.localStats ?? null;
    const quota = usage?.quotaSnapshot ?? null;
    const credential = usage?.credential ?? null;
    const retry = h(Button, { disabled: loading, onClick: () => void load(rangeDays, { refresh: true }), size: "sm", type: "button", variant: "outline" },
      text("retry", "Retry"));

    return h(PageBody, null,
      error ? h(PageNotice, { actions: retry, details: error, detailsLabel: text("errorDetails", "Error details"), tone: "error" },
        text("loadError", "Context7 data could not be loaded.")) : null,
      h(PageSection, {
        actions: [
          h(SelectControl, {
            ariaLabel: text("usageRange", "Local range"),
            disabled: loading,
            key: "range",
            onValueChange: (value) => setRangeDays(Number(value)),
            options: RANGE_OPTIONS.map((value) => ({ value: String(value), label: formatUnit(value, "day") })),
            value: String(rangeDays),
          }),
          h(Button, { disabled: loading, key: "refresh", onClick: () => void load(rangeDays, { refresh: true }), size: "sm", type: "button", variant: "outline" },
            text("refresh", "Refresh")),
        ],
        description: text("localUsageNotice", "Local statistics include only requests made by this Vulcan Code plugin."),
        title: text("localStatsTitle", "Local request statistics"),
      },
        h(MetricGrid, null,
          h(MetricCard, { icon: "requests", label: text("requests", "Requests"), value: count(stats?.requests) }),
          h(MetricCard, { icon: "success", label: text("successes", "Successful"), value: count(stats?.successes) }),
          h(MetricCard, { icon: "activity", label: text("failures", "Failed"), value: count(stats?.failures) }),
          h(MetricCard, {
            icon: "duration",
            label: text("averageDuration", "Average duration"),
            value: stats && stats.requests > 0 ? formatUnit(Math.round(stats.averageDurationMs), "millisecond") : "—",
          }),
        ),
      ),
      h(PageSection, {
        description: text("officialUsageNotice", "Complete account usage, billing periods, seats, and cost remain available in the official Context7 dashboard."),
        footer: [
          officialLink(CONTEXT7_DASHBOARD_URL, text("dashboardLink", "Context7 dashboard")),
          officialLink(CONTEXT7_USAGE_DOCS_URL, text("usageDocsLink", "API and usage documentation")),
        ],
        status: credential ? h(Badge, { variant: credential.configured ? "default" : "outline" },
          credential.configured ? text("apiKeyMode", "API key mode") : text("anonymousMode", "Anonymous mode")) : null,
        title: text("quotaTitle", "Latest Context7 quota snapshot"),
      },
        quota ? h(MetricGrid, null,
          h(MetricCard, { label: text("quotaTier", "Tier"), value: quota.quotaTier ?? "—" }),
          h(MetricCard, { label: text("quotaRemaining", "Remaining"), value: count(quota.quotaRemaining) }),
          h(MetricCard, { label: text("quotaLimit", "Limit"), value: count(quota.quotaLimit) }),
          h(MetricCard, {
            icon: "calendar",
            label: text("quotaReset", "Reset"),
            value: typeof quota.quotaResetUnixSec === "number" ? formatQuotaResetDate(quota.quotaResetUnixSec) : "—",
          }),
        ) : h(PageEmptyState, { title: text("noQuota", "No quota response has been observed for the current authentication generation.") }),
      ),
    );
  }

  return Context7UsagePage;
}
