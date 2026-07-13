// RTK_GAIN_STATS_CACHE_MS keeps the existing statistics snapshot for thirty minutes.
// RTK_GAIN_STATS_CACHE_MS 将已有统计快照保留三十分钟。
const RTK_GAIN_STATS_CACHE_MS = 30 * 60 * 1000;

// rtkGainStatsCache survives tab switches because the enabled plugin module remains mounted.
// rtkGainStatsCache 会跨页签切换保留，因为已启用插件模块保持挂载。
let rtkGainStatsCache = null;

// DEFAULT_RTK_ROUTES mirrors the curated RTK v0.42.4 route set.
// DEFAULT_RTK_ROUTES 对齐 RTK v0.42.4 精选路由集合。
const DEFAULT_RTK_ROUTES = ["cargo", "find", "git", "go", "grep", "jest", "ls", "mypy", "npm", "npx", "pnpm", "pytest", "read", "ruff", "tree", "tsc", "vitest", "wc"];

// RTK_AVAILABLE_ROUTES lists every route exposed by the bundled RTK rewrite command.
// RTK_AVAILABLE_ROUTES 列出随包 RTK rewrite 命令暴露的全部路由。
const RTK_AVAILABLE_ROUTES = [
  "ansible-playbook", "aws", "brew", "bundle", "cargo", "composer", "curl", "df", "diff", "docker",
  "dotnet", "du", "fail2ban-client", "find", "gcloud", "gh", "git", "glab", "go", "golangci-lint",
  "gradlew", "grep", "gt", "hadolint", "helm", "iptables", "jest", "kubectl", "lint", "liquibase",
  "ls", "make", "markdownlint", "mix", "mvn", "mypy", "next", "npm", "npx", "oc", "ping", "pio",
  "pip", "playwright", "pnpm", "poetry", "pre-commit", "prettier", "prisma", "ps", "psql", "pulumi",
  "pytest", "quarto", "rake", "read", "rg", "rspec", "rsync", "rubocop", "ruff", "shellcheck", "shopify",
  "sops", "swift", "systemctl", "terraform", "tofu", "tree", "trunk", "tsc", "uv", "vitest", "wc",
  "wget", "yamllint",
];

// DEFAULT_RTK_ROUTE_SET provides constant-time curated-route labels.
// DEFAULT_RTK_ROUTE_SET 为精选路由标签提供常数时间查找。
const DEFAULT_RTK_ROUTE_SET = new Set(DEFAULT_RTK_ROUTES);

// Create the RTK settings panel through the host-provided stable UI SDK.
// 通过宿主提供的稳定 UI SDK 创建 RTK 设置面板。
//
// sdk supplies host React, design-system components, locale formatting, translations, and plugin-bound actions.
// sdk 提供宿主 React、设计系统组件、语言格式化、翻译与插件绑定 Action。
// Returns one React component integrated into the dedicated plugin settings entry.
// 返回一个集成到插件独立设置入口的 React 组件。
export function createPluginPanel(sdk) {
  // React is the exact host renderer instance and must not be bundled by the plugin.
  // React 是精确的宿主渲染器实例，插件不得自行打包。
  const React = sdk.React;
  // Badge is the shared Vulcan design-system badge component.
  // Badge 是 Vulcan 共享设计系统徽标组件。
  const Badge = sdk.components.Badge;
  // StatusPill is the shared Vulcan lifecycle status component.
  // StatusPill 是 Vulcan 共享生命周期状态组件。
  const StatusPill = sdk.components.StatusPill;
  // text resolves one plugin-owned locale key and retains a safe English source fallback.
  // text 解析一个插件自有语言键，并保留安全的英语源码回退。
  const text = (key, fallback) => sdk.t(key, fallback);

  // numberField reads one RTK metric while accepting only its documented snake-case and host-normalized camel-case names.
  // numberField 读取一个 RTK 指标，仅接受其文档化蛇形名称和宿主规范化驼峰名称。
  //
  // record is one RTK JSON object and names are the exact protocol field names.
  // record 是一个 RTK JSON 对象，names 是精确协议字段名。
  // Returns a finite numeric value or zero.
  // 返回有限数值，否则返回零。
  function numberField(record, ...names) {
    for (const name of names) {
      const value = Number(record?.[name]);
      if (Number.isFinite(value)) {
        return value;
      }
    }
    return 0;
  }

  // checkedRoutesFromSettings restores route checkbox state from the exact persisted routing mode.
  // checkedRoutesFromSettings 根据精确持久化路由模式恢复路由复选状态。
  //
  // settings is the schema-validated plugin configuration.
  // settings 是通过 Schema 校验的插件配置。
  // Returns one route-name set used by the table.
  // 返回路由表使用的路由名集合。
  function checkedRoutesFromSettings(settings) {
    if (settings.routeMode === "disabled") {
      return new Set();
    }
    if (settings.routeMode === "all") {
      const disabled = new Set(settings.disabledRoutes);
      return new Set(RTK_AVAILABLE_ROUTES.filter((route) => !disabled.has(route)));
    }
    return new Set(settings.enabledRoutes);
  }

  // normalizedRoutes converts one draft route set into a stable sorted schema value.
  // normalizedRoutes 将一个草稿路由集合转换为稳定排序的 Schema 值。
  //
  // routes contains checkbox-selected route names.
  // routes 包含复选框选中的路由名。
  // Returns a sorted unique route array.
  // 返回排序且去重的路由数组。
  function normalizedRoutes(routes) {
    return Array.from(routes).map((route) => route.trim().toLowerCase()).filter(Boolean).sort();
  }

  // formatCompactNumber renders large RTK metrics in the inherited Vulcan locale.
  // formatCompactNumber 使用继承的 Vulcan 语言渲染较大的 RTK 指标。
  //
  // value is the finite numeric metric.
  // value 是有限数值指标。
  // Returns compact locale-aware text.
  // 返回紧凑的本地化文本。
  function formatCompactNumber(value) {
    return new Intl.NumberFormat(sdk.locale, { notation: "compact", maximumFractionDigits: 1 }).format(value);
  }

  // formatPercent renders one savings percentage with one decimal place.
  // formatPercent 以一位小数渲染一个节省百分比。
  //
  // value is the percentage metric.
  // value 是百分比指标。
  // Returns percent text.
  // 返回百分比文本。
  function formatPercent(value) {
    return `${value.toFixed(1)}%`;
  }

  // formatDurationMs renders RTK command duration values in compact milliseconds or seconds.
  // formatDurationMs 将 RTK 命令耗时渲染为紧凑毫秒或秒文本。
  //
  // value is the average duration in milliseconds.
  // value 是平均毫秒耗时。
  // Returns compact duration text.
  // 返回紧凑耗时文本。
  function formatDurationMs(value) {
    if (value < 1000) return `${Math.round(value)}ms`;
    if (value < 60000) return `${(value / 1000).toFixed(1)}s`;
    return `${Math.floor(value / 60000)}m${Math.floor((value % 60000) / 1000)}s`;
  }

  // formatStatisticsTimestamp renders one RTK statistics timestamp in the inherited locale.
  // formatStatisticsTimestamp 使用继承的语言区域渲染单个 RTK 统计时间戳。
  //
  // value is one Unix timestamp in milliseconds or an absent value.
  // value 是一个 Unix 毫秒时间戳或空值。
  // Returns localized date-time text or the plugin-owned empty label.
  // 返回本地化日期时间文本或插件自有空值文案。
  function formatStatisticsTimestamp(value) {
    if (!Number.isFinite(value) || value <= 0) {
      return text("stats.none", "None");
    }
    return new Intl.DateTimeFormat(sdk.locale, {
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(value));
  }

  // periodPoints selects and normalizes the requested RTK statistics series.
  // periodPoints 选择并规范化请求的 RTK 统计序列。
  //
  // statistics is the action response and view selects daily, weekly, or monthly data.
  // statistics 是 Action 响应，view 选择日、周或月数据。
  // Returns chart-ready points without mutating the action payload.
  // 返回不修改 Action 载荷的图表点。
  function periodPoints(statistics, view) {
    const source = Array.isArray(statistics?.[view]) ? statistics[view] : [];
    return source.map((point) => {
      const label = view === "daily"
        ? String(point.date ?? "").slice(5)
        : view === "weekly"
          ? `${String(point.week_start ?? point.weekStart ?? "").slice(5)}-${String(point.week_end ?? point.weekEnd ?? "").slice(5)}`
          : String(point.month ?? "");
      return {
        label,
        commands: numberField(point, "commands"),
        savedTokens: numberField(point, "saved_tokens", "savedTokens"),
        savingsPct: numberField(point, "savings_pct", "savingsPct"),
      };
    });
  }

  // RtkGainChart renders the original compact period chart without external libraries.
  // RtkGainChart 在不引入外部库的情况下渲染原有紧凑周期图。
  //
  // props.points contains normalized chart points.
  // props.points 包含规范化图表点。
  // Returns one inherited-theme bar chart.
  // 返回一个继承主题的柱状图。
  function RtkGainChart(props) {
    const points = props.points.slice(-14);
    const maximum = Math.max(1, ...points.map((point) => point.savedTokens));
    if (points.length === 0) {
      return React.createElement("div", { className: "rtk-gain-empty" }, text("stats.empty", "No statistics yet"));
    }
    return React.createElement("div", { className: "plugin-stat-chart", role: "img", "aria-label": text("stats.chartLabel", "RTK token savings trend") },
      ...points.map((point) => React.createElement("div", {
        className: "plugin-stat-chart-column",
        key: point.label,
        title: `${point.label}: ${sdk.formatNumber(point.savedTokens)} · ${formatPercent(point.savingsPct)}`,
      },
      React.createElement("span", { style: { height: `${Math.max(3, (point.savedTokens / maximum) * 100)}%` } }),
      React.createElement("small", null, point.label))),
    );
  }

  // RtkCommandTable renders RTK's by-command savings breakdown.
  // RtkCommandTable 渲染 RTK 按命令分类的节省明细。
  //
  // props.rows contains the exact parsed statistics rows.
  // props.rows 包含精确解析的统计行。
  // Returns a compact table or an explicit empty state.
  // 返回紧凑表格或明确的空状态。
  function RtkCommandTable(props) {
    if (props.rows.length === 0) {
      return React.createElement("div", { className: "rtk-gain-empty" }, text("stats.noCommandBreakdown", "No command breakdown yet"));
    }
    // maximumSavedTokens normalizes the original relative-impact indicator.
    // maximumSavedTokens 归一化原有相对影响指标。
    const maximumSavedTokens = Math.max(1, ...props.rows.map((row) => numberField(row, "saved_tokens", "savedTokens")));
    return React.createElement("section", { className: "rtk-command-stats" },
      React.createElement("div", { className: "rtk-command-stats-header" }, React.createElement("strong", null, text("stats.byCommand", "By command")), React.createElement("span", null, `Top ${props.rows.length}`)),
      React.createElement("div", { className: "rtk-command-table-wrap" },
      React.createElement("table", { className: "rtk-command-table" },
        React.createElement("thead", null, React.createElement("tr", null,
          React.createElement("th", null, "#"),
          React.createElement("th", null, text("stats.command", "Command")),
          React.createElement("th", null, text("stats.count", "Count")),
          React.createElement("th", null, text("stats.saved", "Tokens saved")),
          React.createElement("th", null, text("stats.savingsRate", "Savings")),
          React.createElement("th", null, text("stats.time", "Time")),
          React.createElement("th", null, text("stats.impact", "Impact")),
        )),
        React.createElement("tbody", null, ...props.rows.map((row, index) => React.createElement("tr", { key: `${row.command ?? "command"}:${index}` },
          React.createElement("td", null, index + 1),
          React.createElement("td", null, React.createElement("strong", { title: row.command ?? "" }, row.command ?? "-")),
          React.createElement("td", null, sdk.formatNumber(numberField(row, "count", "commands"))),
          React.createElement("td", null, sdk.formatNumber(numberField(row, "saved_tokens", "savedTokens"))),
          React.createElement("td", null, formatPercent(numberField(row, "avg_savings_pct", "avgSavingsPct", "savings_pct", "savingsPct"))),
          React.createElement("td", null, formatDurationMs(numberField(row, "avg_time_ms", "avgTimeMs"))),
          React.createElement("td", null, React.createElement("span", { className: "rtk-command-impact" }, React.createElement("span", { style: { width: `${Math.max(4, (numberField(row, "saved_tokens", "savedTokens") / maximumSavedTokens) * 100)}%` } }))),
        ))),
      ),
      ),
    );
  }

  // RtkPluginPanel owns plugin-local settings, cache, and view state while the plugin remains enabled.
  // RtkPluginPanel 在插件保持启用期间持有插件本地设置、缓存与视图状态。
  //
  // Returns the complete RTK configuration and statistics page.
  // 返回完整的 RTK 配置与统计页面。
  function RtkPluginPanel() {
    // settings stores the complete schema-validated plugin configuration.
    // settings 保存完整且通过 Schema 校验的插件配置。
    const [settings, setSettings] = React.useState(null);
    // routeMode stores the visible routing-mode draft.
    // routeMode 保存可见的路由模式草稿。
    const [routeMode, setRouteMode] = React.useState("selected");
    // checkedRoutes stores the independently editable route checkbox draft.
    // checkedRoutes 保存可独立编辑的路由复选草稿。
    const [checkedRoutes, setCheckedRoutes] = React.useState(new Set(DEFAULT_RTK_ROUTES));
    // statistics stores the latest cached or freshly loaded RTK gain response.
    // statistics 保存最近缓存或新加载的 RTK gain 响应。
    const [statistics, setStatistics] = React.useState(rtkGainStatsCache?.statistics ?? null);
    // statisticsLoadedAtMs stores the frontend cache timestamp shown by the original RTK page.
    // statisticsLoadedAtMs 保存原 RTK 页面展示的前端缓存时间戳。
    const [statisticsLoadedAtMs, setStatisticsLoadedAtMs] = React.useState(rtkGainStatsCache?.loadedAtMs ?? null);
    // statisticsView selects the daily, weekly, or monthly chart series.
    // statisticsView 选择日、周或月图表序列。
    const [statisticsView, setStatisticsView] = React.useState("daily");
    // statisticsLoading reports the independent statistics request state.
    // statisticsLoading 报告独立统计请求状态。
    const [statisticsLoading, setStatisticsLoading] = React.useState(false);
    // settingsLoading reports the independent configuration request state.
    // settingsLoading 报告独立配置请求状态。
    const [settingsLoading, setSettingsLoading] = React.useState(true);
    // saving reports the atomic configuration write state.
    // saving 报告原子配置写入状态。
    const [saving, setSaving] = React.useState(false);
    // saved provides short-lived successful-save feedback.
    // saved 提供短时保存成功反馈。
    const [saved, setSaved] = React.useState(false);
    // error stores the latest bounded action diagnostic.
    // error 保存最近一次有边界的 Action 诊断。
    const [error, setError] = React.useState("");

    // loadSettings loads only configuration so statistics can never block the settings surface.
    // loadSettings 仅加载配置，确保统计绝不会阻塞设置界面。
    //
    // Returns a promise settled after the exact plugin action completes.
    // 返回在精确插件 Action 完成后结束的 Promise。
    const loadSettings = React.useCallback(async () => {
      setSettingsLoading(true);
      try {
        const nextSettings = await sdk.invoke("settings-get", {});
        setSettings(nextSettings);
        setRouteMode(nextSettings.routeMode);
        setCheckedRoutes(checkedRoutesFromSettings(nextSettings));
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        setSettingsLoading(false);
      }
    }, []);

    // loadStatistics reuses the thirty-minute module cache unless the user explicitly refreshes.
    // loadStatistics 复用三十分钟模块缓存，除非用户明确刷新。
    //
    // force bypasses only the frontend cache and does not alter RTK's own statistics store.
    // force 仅绕过前端缓存，不改变 RTK 自身统计存储。
    // Returns a promise settled after cache selection or the statistics action.
    // 返回在缓存选择或统计 Action 完成后结束的 Promise。
    const loadStatistics = React.useCallback(async (force) => {
      if (!force && rtkGainStatsCache && Date.now() - rtkGainStatsCache.loadedAtMs <= RTK_GAIN_STATS_CACHE_MS) {
        setStatistics(rtkGainStatsCache.statistics);
        setStatisticsLoadedAtMs(rtkGainStatsCache.loadedAtMs);
        return;
      }
      setStatisticsLoading(true);
      try {
        const nextStatistics = await sdk.invoke("statistics-get", {});
        // loadedAtMs is shared by the cache entry and visible page metadata.
        // loadedAtMs 由缓存条目与可见页面元数据共享。
        const loadedAtMs = Date.now();
        rtkGainStatsCache = { loadedAtMs, statistics: nextStatistics };
        setStatistics(nextStatistics);
        setStatisticsLoadedAtMs(loadedAtMs);
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        setStatisticsLoading(false);
      }
    }, []);

    // saveSettings persists one complete configuration derived from the visible draft.
    // saveSettings 持久化一份由可见草稿派生的完整配置。
    //
    // Returns a promise settled after atomic host persistence.
    // 返回在宿主原子持久化完成后结束的 Promise。
    const saveSettings = React.useCallback(async () => {
      if (!settings) {
        return;
      }
      setSaving(true);
      setSaved(false);
      setError("");
      const enabledRoutes = routeMode === "disabled" ? [] : normalizedRoutes(checkedRoutes);
      const disabledRoutes = routeMode === "all"
        ? RTK_AVAILABLE_ROUTES.filter((route) => !checkedRoutes.has(route))
        : [];
      try {
        const persisted = await sdk.invoke("settings-update", {
          ...settings,
          routeMode,
          enabledRoutes,
          disabledRoutes,
        });
        setSettings(persisted);
        setSaved(true);
        window.setTimeout(() => setSaved(false), 1200);
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        setSaving(false);
      }
    }, [checkedRoutes, routeMode, settings]);

    React.useEffect(() => {
      void loadSettings();
      void loadStatistics(false);
    }, [loadSettings, loadStatistics]);

    // toggleRoute applies one checkbox change without mutating the current Set instance.
    // toggleRoute 在不修改当前 Set 实例的情况下应用一个复选框变更。
    //
    // route is the exact RTK route and enabled is the next checkbox state.
    // route 是精确 RTK 路由，enabled 是下一复选状态。
    // Returns no value.
    // 不返回值。
    function toggleRoute(route, enabled) {
      setCheckedRoutes((current) => {
        const next = new Set(current);
        if (enabled) next.add(route); else next.delete(route);
        return next;
      });
      if (routeMode === "disabled" && enabled) {
        setRouteMode("selected");
      }
    }

    // summary contains aggregate RTK command and token metrics.
    // summary 包含 RTK 命令与 Token 汇总指标。
    const summary = statistics?.summary ?? {};
    // summaryCards defines the four original context-density KPIs.
    // summaryCards 定义原有的四个上下文密度关键指标。
    const summaryCards = [
      [text("stats.savedTokens", "Tokens saved"), numberField(summary, "total_saved", "totalSaved")],
      [text("stats.averageSavings", "Average savings"), formatPercent(numberField(summary, "avg_savings_pct", "avgSavingsPct"))],
      [text("stats.commands", "Commands"), numberField(summary, "total_commands", "totalCommands")],
      [text("stats.capacityRatio", "Context capacity"), `${(numberField(summary, "total_input", "totalInput") / Math.max(1, numberField(summary, "total_output", "totalOutput"))).toFixed(1)}×`],
    ];
    // chartPoints contains the normalized series selected by the current tab.
    // chartPoints 包含当前页签选择的规范化序列。
    const chartPoints = periodPoints(statistics, statisticsView);
    // byCommand contains the parsed RTK command breakdown.
    // byCommand 包含解析后的 RTK 命令明细。
    const byCommand = Array.isArray(statistics?.byCommand) ? statistics.byCommand : [];

    return React.createElement("section", { className: "panel full-height settings-detail-panel plugin-panel-rtk" },
      React.createElement("div", { className: "settings-panel-title-row" },
        React.createElement("strong", null, text("title", "RTK command routing")),
        React.createElement(Badge, { variant: "outline" }, text("hookBadge", "Declarative hooks")),
      ),
      React.createElement("div", { className: "settings-form-grid" },
        React.createElement("article", { className: "settings-card" },
          React.createElement(StatusPill, {
            status: settings?.enabled ? "ready" : "warning",
            label: settingsLoading ? text("loadingSettings", "Loading settings") : settings?.enabled ? text("enabled", "Enabled") : text("disabled", "Disabled"),
          }),
          React.createElement("strong", null, text("densityTitle", "High-density command context")),
          React.createElement("p", null, text("densityDescription", "RTK compresses noisy test, build, search, and Git output so the AI can retain more useful analysis. Unmatched or failed rewrites fall back to the original command.")),
          settings ? React.createElement(React.Fragment, null,
            React.createElement("label", { className: "settings-checkbox-line" },
              React.createElement("input", { checked: settings.enabled, onChange: (event) => setSettings({ ...settings, enabled: event.currentTarget.checked }), type: "checkbox" }),
              React.createElement("span", null, React.createElement("strong", null, text("enableRouting", "Enable RTK routing")), React.createElement("small", null, text("enableRoutingHelp", "Rewrite supported shell_command calls through the bundled RTK binary."))),
            ),
            React.createElement("label", { className: "settings-checkbox-line" },
              React.createElement("input", { checked: settings.gitCommitContextEnabled, onChange: (event) => setSettings({ ...settings, gitCommitContextEnabled: event.currentTarget.checked }), type: "checkbox" }),
              React.createElement("span", null, React.createElement("strong", null, text("gitContext", "Use RTK for commit generation")), React.createElement("small", null, text("gitContextHelp", "Compress git status and git diff context used to generate commit messages."))),
            ),
            React.createElement("label", { className: "settings-checkbox-line" },
              React.createElement("input", { checked: settings.telemetryDisabled, onChange: (event) => setSettings({ ...settings, telemetryDisabled: event.currentTarget.checked }), type: "checkbox" }),
              React.createElement("span", null, React.createElement("strong", null, text("disableTelemetry", "Disable RTK telemetry environment variable")), React.createElement("small", null, text("disableTelemetryHelp", "Inject RTK_TELEMETRY_DISABLED=1 into rewrite and proxied command processes."))),
            ),
          ) : null,
        ),
        React.createElement("article", { className: "settings-card rtk-gain-panel" },
          React.createElement("div", { className: "rtk-gain-header" },
            React.createElement("div", null,
              React.createElement(StatusPill, { status: statistics?.available ? "ready" : "warning", label: statisticsLoading ? text("stats.loading", "Loading") : statistics?.available ? text("stats.ready", "Stats ready") : text("stats.unavailable", "Unavailable") }),
              React.createElement("strong", null, text("stats.title", "RTK context-density statistics")),
              React.createElement("p", null, text("stats.description", "Token savings are cached for thirty minutes. Switching daily, weekly, and monthly views never sends another request.")),
            ),
            React.createElement("button", { className: "skill-action-button secondary", disabled: statisticsLoading, onClick: () => void loadStatistics(true), type: "button" }, statisticsLoading ? text("stats.refreshing", "Refreshing") : text("stats.refresh", "Refresh")),
          ),
          React.createElement("div", { className: "rtk-gain-meta" },
            React.createElement("span", null, `${text("stats.lastLoaded", "Last loaded")}: ${formatStatisticsTimestamp(statisticsLoadedAtMs)}`),
            React.createElement("span", null, `${text("stats.generated", "Generated")}: ${formatStatisticsTimestamp(numberField(statistics, "generated_at_ms", "generatedAtMs"))}`),
          ),
          statistics?.available ? React.createElement(React.Fragment, null,
            React.createElement("div", { className: "rtk-gain-summary-grid" }, ...summaryCards.map(([label, value]) => React.createElement("div", { className: "rtk-gain-summary-card", key: label }, React.createElement("span", null, label), React.createElement("strong", null, typeof value === "number" ? formatCompactNumber(value) : value)))),
            React.createElement("div", { className: "rtk-gain-tabs" }, ...["daily", "weekly", "monthly"].map((view) => React.createElement("button", { className: `rtk-gain-tab ${statisticsView === view ? "active" : ""}`, key: view, onClick: () => setStatisticsView(view), type: "button" }, text(`stats.${view}`, view)))),
            React.createElement(RtkGainChart, { points: chartPoints }),
            React.createElement(RtkCommandTable, { rows: byCommand }),
          ) : React.createElement("div", { className: "rtk-gain-empty" }, statistics?.error ?? text("stats.unavailableHelp", "Statistics are currently unavailable.")),
        ),
        React.createElement("article", { className: "settings-card" },
          React.createElement("div", { className: "settings-route-header" },
            React.createElement("div", null, React.createElement("strong", null, text("routes.title", "RTK routes")), React.createElement("p", null, text("routes.description", "Choose exactly which supported commands may be rewritten."))),
            React.createElement(Badge, { variant: "outline" }, `${checkedRoutes.size}/${RTK_AVAILABLE_ROUTES.length}`),
          ),
          React.createElement("div", { className: "settings-route-actions" },
            React.createElement("button", { className: "secondary-button", onClick: () => { setRouteMode("selected"); setCheckedRoutes(new Set(DEFAULT_RTK_ROUTES)); }, type: "button" }, text("routes.curated", "Curated")),
            React.createElement("button", { className: "secondary-button", onClick: () => { setRouteMode("all"); setCheckedRoutes(new Set(RTK_AVAILABLE_ROUTES)); }, type: "button" }, text("routes.all", "All")),
            React.createElement("button", { className: "secondary-button", onClick: () => { setRouteMode("disabled"); setCheckedRoutes(new Set()); }, type: "button" }, text("routes.clear", "Clear")),
          ),
          React.createElement("div", { className: "settings-route-table-wrap" },
            React.createElement("table", { className: "settings-route-table" },
              React.createElement("thead", null, React.createElement("tr", null, React.createElement("th", null), React.createElement("th", null, text("routes.command", "Command")), React.createElement("th", null, text("routes.group", "Group")))),
              React.createElement("tbody", null, ...RTK_AVAILABLE_ROUTES.map((route) => {
                const enabled = checkedRoutes.has(route);
                return React.createElement("tr", { "aria-checked": enabled, className: "settings-route-row", key: route, onClick: () => toggleRoute(route, !enabled), onKeyDown: (event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); toggleRoute(route, !enabled); } }, role: "checkbox", tabIndex: 0 },
                  React.createElement("td", null, React.createElement("input", { "aria-label": `${text("routes.enable", "Enable route")} ${route}`, checked: enabled, onChange: (event) => toggleRoute(route, event.currentTarget.checked), onClick: (event) => event.stopPropagation(), type: "checkbox" })),
                  React.createElement("td", null, React.createElement("strong", null, route)),
                  React.createElement("td", null, DEFAULT_RTK_ROUTE_SET.has(route) ? text("routes.curated", "Curated") : text("routes.extended", "Extended")),
                );
              })),
            ),
          ),
          React.createElement("small", { className: "settings-muted-line" }, `${checkedRoutes.size} ${text("routes.selected", "RTK routes selected")}`),
        ),
        error ? React.createElement("p", { className: "plugin-panel-error" }, error) : null,
        React.createElement("div", { className: "settings-card-actions" }, React.createElement("button", { className: "skill-action-button", disabled: saving || !settings, onClick: () => void saveSettings(), type: "button" }, saving ? text("saving", "Saving") : saved ? text("saved", "Saved") : text("save", "Save"))),
      ),
    );
  }

  return RtkPluginPanel;
}
