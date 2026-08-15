// ALL_CONSUMERS identifies the unfiltered system statistics projection.
// ALL_CONSUMERS 标识未筛选的系统统计投影。
const ALL_CONSUMERS = "__all_consumers__";

// MAX_DAILY_ROWS bounds the detailed aggregate table rendered in one panel pass.
// MAX_DAILY_ROWS 限制单次面板渲染的每日聚合明细数量。
const MAX_DAILY_ROWS = 100;

// MAX_RECENT_ATTEMPTS bounds the physical attempt evidence rendered in one panel pass.
// MAX_RECENT_ATTEMPTS 限制单次面板渲染的物理请求证据数量。
const MAX_RECENT_ATTEMPTS = 50;

// DECIMAL_PRICE_PATTERN mirrors the host-validated plugin configuration contract.
// DECIMAL_PRICE_PATTERN 镜像由宿主校验的插件配置契约。
const DECIMAL_PRICE_PATTERN = /^(0|[1-9][0-9]*)(\.[0-9]+)?$/;

// EMPTY_PRICE_DRAFT preserves unknown model rates as missing rather than zero.
// EMPTY_PRICE_DRAFT 将未知模型费率保持为缺失而非零。
const EMPTY_PRICE_DRAFT = Object.freeze({
  inputPerMillion: "",
  outputPerMillion: "",
  cacheReadPerMillion: "",
  cacheWritePerMillion: "",
});

// PRICE_FIELDS defines the complete editable per-million Token price dimensions.
// PRICE_FIELDS 定义完整的可编辑每百万 Token 价格维度。
const PRICE_FIELDS = Object.freeze([
  { key: "inputPerMillion", messageKey: "pricing.input", fallback: "Uncached input" },
  { key: "outputPerMillion", messageKey: "pricing.output", fallback: "Output" },
  { key: "cacheReadPerMillion", messageKey: "pricing.cacheRead", fallback: "Cache read" },
  { key: "cacheWritePerMillion", messageKey: "pricing.cacheWrite", fallback: "Cache write" },
]);

// Create the Provider usage statistics panel through the stable host UI SDK.
// 通过稳定宿主 UI SDK 创建 Provider 用量统计面板。
//
// sdk supplies host React, design-system components, localization, and authenticated actions.
// sdk 提供宿主 React、设计系统组件、本地化与已认证 Action。
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
  // Button 是 Vulcan 共享紧凑操作按钮。
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
  // EmptyState is the shared host empty-state component.
  // EmptyState 是共享宿主空状态组件。
  const EmptyState = sdk.components.EmptyState;
  // ErrorState is the shared host error-state component.
  // ErrorState 是共享宿主错误状态组件。
  const ErrorState = sdk.components.ErrorState;
  // text resolves one plugin locale key with an explicit English source fallback.
  // text 使用明确英语源码回退解析一个插件语言键。
  const text = (key, fallback) => sdk.t(key, fallback);

  // numeric converts one system statistics scalar into a finite display number.
  // numeric 将一个系统统计标量转换为有限展示数值。
  //
  // value is one JSON numeric scalar from the host dashboard.
  // value 是宿主仪表盘中的一个 JSON 数值标量。
  // Returns a finite number and preserves absence as zero only for aggregate display.
  // 返回有限数值；仅在聚合展示中把缺失值视为零。
  function numeric(value) {
    // number is the exact JavaScript numeric projection.
    // number 是精确的 JavaScript 数值投影。
    const number = Number(value);
    return Number.isFinite(number) ? number : 0;
  }

  // formatInteger renders one count through the inherited host locale.
  // formatInteger 使用继承的宿主语言渲染一个计数。
  //
  // value is one count or token total.
  // value 是一个计数或 Token 合计。
  // Returns one locale-aware integer string.
  // 返回一个遵循当前语言环境的整数文本。
  function formatInteger(value) {
    return new Intl.NumberFormat(sdk.locale, { maximumFractionDigits: 0 }).format(numeric(value));
  }

  // formatTimestamp renders one Unix millisecond timestamp through the inherited host locale.
  // formatTimestamp 使用继承的宿主语言渲染一个 Unix 毫秒时间戳。
  //
  // value is one host-produced Unix millisecond timestamp.
  // value 是宿主产生的一个 Unix 毫秒时间戳。
  // Returns one compact local date and time string.
  // 返回一个紧凑的本地日期时间文本。
  function formatTimestamp(value) {
    return new Intl.DateTimeFormat(sdk.locale, {
      dateStyle: "short",
      timeStyle: "medium",
    }).format(new Date(numeric(value)));
  }

  // formatReferenceEstimate renders one plugin-computed reference estimate.
  // formatReferenceEstimate 渲染一个由插件计算的参考估算值。
  //
  // record contains ephemeral estimate fields attached by the plugin runtime.
  // record 包含由插件运行时附加的临时估算字段。
  // Returns an explicitly estimated value or the exact reason it is unavailable.
  // 返回带明确估算标记的值，或返回其不可用的精确原因。
  function formatReferenceEstimate(record) {
    // currency is the plugin-catalog currency and is never persisted by the core.
    // currency 是插件价目表币种，内核绝不会持久化它。
    const currency = record.estimatedCurrency ? ` ${record.estimatedCurrency}` : "";
    if (record.estimatedCost !== null && record.estimatedCost !== undefined) {
      return `~${record.estimatedCost}${currency}`;
    }
    if (record.pricingMatchStatus === "unmatched") {
      return text("pricing.unmatched", "Model not in reference catalog");
    }
    if (record.pricingMatchStatus === "incomplete_usage") {
      return text("pricing.incomplete", "Incomplete Token facts");
    }
    return "—";
  }

  // summarizeDaily derives display-only KPIs from host-owned daily facts.
  // summarizeDaily 从宿主持有的每日事实派生纯展示 KPI。
  //
  // rows contains the exact consumer-filtered native daily aggregates.
  // rows 包含精确的消费者筛选后原生每日聚合。
  // Returns physical attempts, success coverage, reported-usage coverage, and known token totals.
  // 返回物理请求数、成功率、报告用量覆盖率与已知 Token 合计。
  function summarizeDaily(rows) {
    // attempts counts physical upstream attempts, including retries and failures.
    // attempts 统计真实上游请求，包括重试与失败。
    const attempts = rows.reduce((sum, row) => sum + numeric(row.attemptCount), 0);
    // successes counts terminally successful physical attempts.
    // successes 统计终态成功的物理请求。
    const successes = rows.reduce((sum, row) => sum + numeric(row.successfulAttemptCount), 0);
    // reported counts attempts carrying direct Provider usage facts.
    // reported 统计携带 Provider 直接用量事实的请求。
    const reported = rows.reduce((sum, row) => sum + numeric(row.reportedUsageAttemptCount), 0);
    // totalTokens sums known normalized totals without fabricating missing rows.
    // totalTokens 汇总已知规范化 Token，不为缺失行伪造数值。
    const totalTokens = rows.reduce((sum, row) => sum + numeric(row.totalTokens), 0);
    return {
      attempts,
      successes,
      reported,
      totalTokens,
      successRate: attempts > 0 ? (successes / attempts) * 100 : null,
      usageCoverage: attempts > 0 ? (reported / attempts) * 100 : null,
    };
  }

  // editablePrices resolves one model's override first and catalog defaults second.
  // editablePrices 优先解析模型覆盖，其次解析目录默认值。
  //
  // settings is the schema-validated plugin settings projection and modelId is exact.
  // settings 是通过 Schema 校验的插件设置投影，modelId 是精确模型标识。
  // Returns an isolated editable price object or blank unknown fields.
  // 返回隔离的可编辑价格对象，或返回空白未知字段。
  function editablePrices(settings, modelId) {
    const configured = settings?.modelOverrides?.[modelId];
    const catalog = settings?.catalogModels?.find((entry) => entry.modelId === modelId)?.prices;
    return { ...(configured ?? catalog ?? EMPTY_PRICE_DRAFT) };
  }

  // pricingDraftIsValid mirrors the exact non-negative decimal configuration rules.
  // pricingDraftIsValid 镜像精确的非负十进制配置规则。
  //
  // draft contains all four per-million Token price strings.
  // draft 包含四个每百万 Token 价格字符串。
  // Returns true only when every required price is valid.
  // 仅当全部必需价格均有效时返回 true。
  function pricingDraftIsValid(draft) {
    return Object.values(draft).every(
      (value) => typeof value === "string" && value.length <= 64 && DECIMAL_PRICE_PATTERN.test(value),
    );
  }

  // UsageStatisticsPanel combines host-owned Token facts with an in-memory plugin price catalog.
  // UsageStatisticsPanel 将宿主持有的 Token 事实与插件内存价目表组合展示。
  function UsageStatisticsPanel() {
    // dashboard stores the latest schema-validated host statistics snapshot.
    // dashboard 保存最近一份通过 Schema 校验的宿主统计快照。
    const [dashboard, setDashboard] = React.useState(null);
    // pricingSettings stores plugin-owned model overrides and the bundled catalog projection.
    // pricingSettings 保存插件自有模型覆盖与内置目录投影。
    const [pricingSettings, setPricingSettings] = React.useState(null);
    // selectedPricingModel identifies the exact model edited by the pricing form.
    // selectedPricingModel 标识价格表单当前编辑的精确模型。
    const [selectedPricingModel, setSelectedPricingModel] = React.useState("");
    // pricingDraft stores four exact decimal strings without numeric conversion.
    // pricingDraft 保存四个不经过数值转换的精确十进制字符串。
    const [pricingDraft, setPricingDraft] = React.useState({ ...EMPTY_PRICE_DRAFT });
    // settingsBusy prevents overlapping model-price persistence operations.
    // settingsBusy 防止模型价格持久化操作重叠。
    const [settingsBusy, setSettingsBusy] = React.useState(false);
    // consumerFilter selects one exact server-bound consumer identity.
    // consumerFilter 选择一个精确的服务端消费者身份。
    const [consumerFilter, setConsumerFilter] = React.useState(ALL_CONSUMERS);
    // loading prevents overlapping dashboard reads.
    // loading 防止仪表盘读取重叠。
    const [loading, setLoading] = React.useState(true);
    // error stores the latest bounded host query diagnostic.
    // error 保存最近一条有界宿主查询诊断。
    const [error, setError] = React.useState("");
    // clearPreview stores the revision-bound destructive maintenance preview.
    // clearPreview 保存绑定修订的破坏性维护预览。
    const [clearPreview, setClearPreview] = React.useState(null);
    // clearBusy prevents concurrent preview and apply operations.
    // clearBusy 防止预览与应用操作并发执行。
    const [clearBusy, setClearBusy] = React.useState(false);
    // notice stores the latest successful maintenance result.
    // notice 保存最近一次维护成功结果。
    const [notice, setNotice] = React.useState("");
    // mountedRef prevents a completed host action from mutating an unmounted plugin node.
    // mountedRef 防止已完成宿主 Action 修改已经卸载的插件节点。
    const mountedRef = React.useRef(true);

    // refresh loads one coherent native dashboard revision through the plugin action boundary.
    // refresh 通过插件 Action 边界加载一个一致的原生仪表盘修订版。
    const refresh = React.useCallback(async () => {
      if (loading) return;
      setLoading(true);
      setError("");
      try {
        const [nextDashboard, nextSettings] = await Promise.all([
          sdk.invoke("dashboard-get", {}),
          sdk.invoke("settings-get", {}),
        ]);
        if (mountedRef.current) {
          setDashboard(nextDashboard);
          setPricingSettings(nextSettings);
          const nextModel = selectedPricingModel || nextSettings.catalogModels[0]?.modelId || "";
          setSelectedPricingModel(nextModel);
          setPricingDraft(editablePrices(nextSettings, nextModel));
        }
      } catch (reason) {
        if (mountedRef.current) setError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        if (mountedRef.current) setLoading(false);
      }
    }, [loading, selectedPricingModel]);

    // selectPricingModel loads one exact override or catalog default into the string-only editor.
    // selectPricingModel 将一个精确覆盖或目录默认值加载到纯字符串编辑器。
    const selectPricingModel = React.useCallback((modelId) => {
      setSelectedPricingModel(modelId);
      setPricingDraft(editablePrices(pricingSettings, modelId));
      setNotice("");
    }, [pricingSettings]);

    // persistPricingOverrides saves one complete plugin-owned configuration and refreshes estimates.
    // persistPricingOverrides 保存完整插件自有配置并刷新估算结果。
    const persistPricingOverrides = React.useCallback(async (modelOverrides, successMessage) => {
      if (settingsBusy) return;
      setSettingsBusy(true);
      setError("");
      setNotice("");
      try {
        const nextSettings = await sdk.invoke("settings-update", { modelOverrides });
        const nextDashboard = await sdk.invoke("dashboard-get", {});
        if (mountedRef.current) {
          setPricingSettings(nextSettings);
          setDashboard(nextDashboard);
          setPricingDraft(editablePrices(nextSettings, selectedPricingModel));
          setNotice(successMessage);
        }
      } catch (reason) {
        if (mountedRef.current) setError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        if (mountedRef.current) setSettingsBusy(false);
      }
    }, [selectedPricingModel, settingsBusy]);

    // savePricingOverride validates and persists all four exact prices for the selected model.
    // savePricingOverride 校验并持久化所选模型的四项精确价格。
    const savePricingOverride = React.useCallback(() => {
      if (!pricingSettings || !selectedPricingModel || !pricingDraftIsValid(pricingDraft)) return;
      void persistPricingOverrides({
        ...pricingSettings.modelOverrides,
        [selectedPricingModel]: { ...pricingDraft },
      }, text("pricing.saved", "Saved model price override"));
    }, [persistPricingOverrides, pricingDraft, pricingSettings, selectedPricingModel]);

    // resetPricingOverride removes only the selected model override and restores catalog matching.
    // resetPricingOverride 仅移除所选模型覆盖并恢复目录匹配。
    const resetPricingOverride = React.useCallback(() => {
      if (!pricingSettings || !selectedPricingModel) return;
      const nextOverrides = { ...pricingSettings.modelOverrides };
      delete nextOverrides[selectedPricingModel];
      void persistPricingOverrides(nextOverrides, text("pricing.resetDone", "Restored catalog price"));
    }, [persistPricingOverrides, pricingSettings, selectedPricingModel]);

    // previewClear reads the exact destructive scope without modifying system statistics.
    // previewClear 读取精确破坏范围，且不修改系统统计。
    const previewClear = React.useCallback(async () => {
      if (clearBusy) return;
      setClearBusy(true);
      setError("");
      setNotice("");
      try {
        const preview = await sdk.invoke("statistics-clear-preview", {});
        if (mountedRef.current) setClearPreview(preview);
      } catch (reason) {
        if (mountedRef.current) setError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        if (mountedRef.current) setClearBusy(false);
      }
    }, [clearBusy]);

    // applyClear deletes history only when the preview revision is still current.
    // applyClear 仅在预览修订仍然有效时删除历史。
    const applyClear = React.useCallback(async () => {
      if (clearBusy || !clearPreview) return;
      setClearBusy(true);
      setError("");
      try {
        const result = await sdk.invoke("statistics-clear-apply", {
          expectedAggregateRevision: clearPreview.aggregateRevision,
        });
        const nextDashboard = await sdk.invoke("dashboard-get", {});
        if (mountedRef.current) {
          setDashboard(nextDashboard);
          setClearPreview(null);
          setNotice(`${text("clear.success", "Cleared historical statistics")}: ${formatInteger(result.clearedAttemptCount)} ${text("clear.attempts", "attempts")}`);
        }
      } catch (reason) {
        if (mountedRef.current) setError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        if (mountedRef.current) setClearBusy(false);
      }
    }, [clearBusy, clearPreview]);

    React.useEffect(() => {
      mountedRef.current = true;
      // The initial read bypasses the overlap guard because loading starts true.
      // 初始读取绕过重叠保护，因为 loading 初始值为 true。
      void Promise.all([
        sdk.invoke("dashboard-get", {}),
        sdk.invoke("settings-get", {}),
      ]).then(([nextDashboard, nextSettings]) => {
        if (mountedRef.current) {
          setDashboard(nextDashboard);
          setPricingSettings(nextSettings);
          const initialModel = nextSettings.catalogModels[0]?.modelId || "";
          setSelectedPricingModel(initialModel);
          setPricingDraft(editablePrices(nextSettings, initialModel));
        }
      }).catch((reason) => {
        if (mountedRef.current) setError(reason instanceof Error ? reason.message : String(reason));
      }).finally(() => {
        if (mountedRef.current) setLoading(false);
      });
      return () => {
        mountedRef.current = false;
      };
    }, []);

    // consumerIds contains exact server-bound identities observed in either dashboard projection.
    // consumerIds 包含任一仪表盘投影中观察到的精确服务端身份。
    const consumerIds = React.useMemo(() => Array.from(new Set([
      ...(dashboard?.daily.map((row) => row.consumerId) ?? []),
      ...(dashboard?.recentAttempts.map((row) => row.consumerId) ?? []),
    ])).sort(), [dashboard]);
    // pricingModelIds merges bundled, observed, and explicitly configured exact model identities.
    // pricingModelIds 合并内置、已观测与显式配置的精确模型标识。
    const pricingModelIds = React.useMemo(() => Array.from(new Set([
      ...(pricingSettings?.catalogModels.map((entry) => entry.modelId) ?? []),
      ...(dashboard?.daily.map((row) => row.modelId) ?? []),
      ...(dashboard?.recentAttempts.map((row) => row.modelId) ?? []),
      ...Object.keys(pricingSettings?.modelOverrides ?? {}),
    ])).sort(), [dashboard, pricingSettings]);
    // selectedModelHasOverride controls the exact restore-default operation.
    // selectedModelHasOverride 控制精确的恢复默认操作。
    const selectedModelHasOverride = Boolean(
      selectedPricingModel && pricingSettings?.modelOverrides?.[selectedPricingModel],
    );
    // pricingDraftValid controls persistence without coercing decimal strings.
    // pricingDraftValid 控制持久化，且不强制转换十进制字符串。
    const pricingDraftValid = pricingDraftIsValid(pricingDraft);

    React.useEffect(() => {
      if (consumerFilter !== ALL_CONSUMERS && !consumerIds.includes(consumerFilter)) {
        setConsumerFilter(ALL_CONSUMERS);
      }
    }, [consumerFilter, consumerIds]);

    // filteredDaily applies the selected consumer without changing persisted aggregates.
    // filteredDaily 应用选定消费者，且不改变持久化聚合。
    const filteredDaily = dashboard?.daily.filter(
      (row) => consumerFilter === ALL_CONSUMERS || row.consumerId === consumerFilter,
    ) ?? [];
    // filteredAttempts applies the same source boundary to exact attempt evidence.
    // filteredAttempts 对精确请求证据应用相同来源边界。
    const filteredAttempts = dashboard?.recentAttempts.filter(
      (row) => consumerFilter === ALL_CONSUMERS || row.consumerId === consumerFilter,
    ) ?? [];
    // summary is a display-only projection over already persisted host facts.
    // summary 是对已持久化宿主事实的纯展示投影。
    const summary = summarizeDaily(filteredDaily);

    return h("section", { className: "full-height settings-detail-panel plugin-statistics-layout" },
      h("div", { className: "plugin-statistics-header" },
        h("div", null,
          h("strong", null, text("title", "System Usage Statistics")),
          h("p", null, text("subtitle", "The core stores Token facts only. This plugin computes non-persisted reference estimates from its own catalog.")),
        ),
        h(StatusPill, {
          status: error ? "warning" : "ready",
          label: loading
            ? text("status.loading", "Loading system statistics")
            : error
              ? text("status.error", "System statistics failed to load")
              : text("status.ready", "System statistics available"),
        }),
      ),
      h("div", { className: "plugin-statistics-range-header" },
        h("div", { className: "plugin-statistics-toolbar" },
          h(Button, {
            "aria-busy": loading,
            "aria-label": loading ? text("refreshing", "Refreshing") : text("refresh", "Refresh"),
            className: "plugin-statistics-refresh-button",
            disabled: loading,
            onClick: () => void refresh(),
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
          consumerIds.length > 1 ? h(SelectControl, {
            ariaLabel: text("consumer.label", "Consumer"),
            className: "plugin-statistics-toolbar-select",
            value: consumerFilter,
            options: [
              { value: ALL_CONSUMERS, label: text("consumer.all", "All consumers") },
              ...consumerIds.map((consumerId) => ({ value: consumerId, label: consumerId })),
            ],
            onValueChange: setConsumerFilter,
          }) : null,
          h(Button, {
            "aria-busy": clearBusy,
            disabled: clearBusy || loading,
            onClick: () => void previewClear(),
            size: "sm",
            title: text("clear.preview", "Preview clearing history"),
            type: "button",
            variant: "outline",
          }, clearBusy ? text("clear.previewing", "Preparing preview") : text("clear.preview", "Preview clearing history")),
        ),
        dashboard ? h("div", { className: "plugin-statistics-toolbar" },
          h(Badge, { variant: "outline" }, text("source.host", "Host data")),
          h(Badge, { variant: "outline" },
            `${text("pricing.catalog", "Reference catalog")} · ${dashboard.pricingCatalog.currency} · ${formatInteger(dashboard.pricingCatalog.entryCount)} + ${formatInteger(dashboard.pricingCatalog.overrideCount)}`,
          ),
          h("span", { className: "plugin-statistics-privacy" },
            `${text("revision", "Revision")} ${formatInteger(dashboard.aggregateRevision)} · ${formatTimestamp(dashboard.generatedAtMs)}`,
          ),
        ) : null,
      ),
      error ? h(ErrorState, {
        title: text("error.title", "Unable to read system statistics"),
        description: error,
      }) : null,
      notice ? h("p", { className: "plugin-statistics-privacy" }, notice) : null,
      clearPreview ? h("article", { className: "plugin-statistics-card" },
        h("div", { className: "plugin-statistics-section-header" },
          h("div", null,
            h("strong", null, text("clear.title", "Clear historical statistics")),
            h("p", { className: "plugin-statistics-privacy" },
              `${text("clear.previewSummary", "This will permanently remove")}: ${formatInteger(clearPreview.attemptCount)} ${text("clear.attempts", "attempts")} · ${formatInteger(clearPreview.totalTokens)} ${text("clear.tokens", "tokens")}`,
            ),
          ),
          h("div", { className: "plugin-statistics-toolbar" },
            h(Button, {
              disabled: clearBusy,
              onClick: () => setClearPreview(null),
              size: "sm",
              type: "button",
              variant: "outline",
            }, text("clear.cancel", "Cancel")),
            h(Button, {
              "aria-busy": clearBusy,
              disabled: clearBusy,
              onClick: () => void applyClear(),
              size: "sm",
              type: "button",
            }, clearBusy ? text("clear.applying", "Clearing") : text("clear.apply", "Confirm clear")),
          ),
        ),
      ) : null,
      !dashboard && !error ? h(EmptyState, {
        title: text("status.loading", "Loading system statistics"),
        description: text("loading.description", "Reading the authoritative host statistics database."),
      }) : null,
      dashboard ? h(React.Fragment, null,
        h("div", { className: "plugin-statistics-summary-grid" },
          h(MetricCard, {
            icon: "requests",
            label: text("summary.requests", "Physical attempts"),
            value: formatInteger(summary.attempts),
            detail: text("summary.requestsDetail", "Includes retries and failures"),
          }),
          h(MetricCard, {
            icon: "success",
            label: text("summary.successRate", "Success rate"),
            value: summary.successRate === null ? "—" : `${summary.successRate.toFixed(1)}%`,
            detail: `${formatInteger(summary.successes)} ${text("summary.successes", "successful attempts")}`,
          }),
          h(MetricCard, {
            icon: "tokens",
            label: text("summary.tokens", "Known tokens"),
            value: formatInteger(summary.totalTokens),
            detail: text("summary.tokensDetail", "Unknown usage remains absent"),
          }),
          h(MetricCard, {
            icon: "activity",
            label: text("summary.usageCoverage", "Usage coverage"),
            value: summary.usageCoverage === null ? "—" : `${summary.usageCoverage.toFixed(1)}%`,
            detail: text("summary.usageCoverageDetail", "Directly Provider-reported"),
          }),
        ),
        h("article", { className: "plugin-statistics-card" },
          h("div", { className: "plugin-statistics-section-header" },
            h("div", null,
              h("strong", null, text("pricing.title", "Model reference prices")),
              h("p", { className: "plugin-statistics-privacy" },
                text("pricing.description", "Overrides belong to this plugin configuration. Estimates are recomputed and never written to the statistics database."),
              ),
            ),
            h(Badge, { variant: "outline" }, selectedModelHasOverride
              ? text("pricing.override", "Custom override")
              : text("pricing.default", "Catalog default")),
          ),
          pricingModelIds.length === 0 ? h(EmptyState, {
            title: text("pricing.empty", "No model identities available"),
          }) : h(React.Fragment, null,
            h(SelectControl, {
              ariaLabel: text("pricing.model", "Model"),
              className: "plugin-statistics-pricing-model",
              value: selectedPricingModel,
              options: pricingModelIds.map((modelId) => ({ value: modelId, label: modelId })),
              onValueChange: selectPricingModel,
            }),
            h("div", { className: "plugin-statistics-price-grid" },
              ...PRICE_FIELDS.map((field) => h("label", {
                className: "plugin-statistics-price-field",
                key: field.key,
              },
                h("span", null, `${text(field.messageKey, field.fallback)} · ${text("pricing.perMillion", "per 1M Token")}`),
                h("input", {
                  "aria-label": text(field.messageKey, field.fallback),
                  className: "plugin-statistics-price-input",
                  disabled: settingsBusy,
                  inputMode: "decimal",
                  maxLength: 64,
                  onChange: (event) => setPricingDraft((current) => ({
                    ...current,
                    [field.key]: event.currentTarget.value.trim(),
                  })),
                  spellCheck: false,
                  type: "text",
                  value: pricingDraft[field.key],
                }),
              )),
            ),
            h("div", { className: "plugin-statistics-toolbar" },
              h(Button, {
                "aria-busy": settingsBusy,
                disabled: settingsBusy || !selectedPricingModel || !pricingDraftValid,
                onClick: savePricingOverride,
                size: "sm",
                type: "button",
              }, settingsBusy ? text("pricing.saving", "Saving") : text("pricing.save", "Save override")),
              h(Button, {
                disabled: settingsBusy || !selectedModelHasOverride,
                onClick: resetPricingOverride,
                size: "sm",
                type: "button",
                variant: "outline",
              }, text("pricing.reset", "Restore catalog default")),
              !pricingDraftValid ? h("span", { className: "plugin-statistics-privacy" },
                text("pricing.validation", "Enter four non-negative decimal prices."),
              ) : null,
            ),
          ),
        ),
        h("article", { className: "plugin-statistics-card" },
          h("div", { className: "plugin-statistics-section-header" },
            h("strong", null, text("daily.title", "Daily model usage")),
            h(Badge, { variant: "outline" }, `${Math.min(filteredDaily.length, MAX_DAILY_ROWS)} / ${filteredDaily.length}`),
          ),
          filteredDaily.length === 0 ? h(EmptyState, {
            title: text("daily.empty", "No daily usage facts"),
          }) : h("div", { className: "plugin-statistics-table-wrap" },
            h("table", { className: "plugin-statistics-table" },
              h("thead", null, h("tr", null,
                h("th", null, text("daily.date", "Date")),
                h("th", null, text("daily.provider", "Provider / model")),
                h("th", null, text("daily.consumer", "Consumer")),
                h("th", null, text("daily.operation", "Operation")),
                h("th", null, text("daily.attempts", "Attempts")),
                h("th", null, text("daily.tokens", "Tokens")),
                h("th", null, text("daily.cost", "Reference estimate")),
              )),
              h("tbody", null, ...filteredDaily.slice(0, MAX_DAILY_ROWS).map((row) => h("tr", {
                key: `${row.calendarDate}/${row.providerId}/${row.serviceEntryId}/${row.consumerId}/${row.operation}/${row.modelId}`,
              },
                h("td", null, row.calendarDate),
                h("td", null, `${row.providerId} / ${row.modelId}`),
                h("td", null, row.consumerId),
                h("td", null, row.operation),
                h("td", null, `${formatInteger(row.successfulAttemptCount)} / ${formatInteger(row.attemptCount)}`),
                h("td", null, row.totalTokens === null ? "—" : formatInteger(row.totalTokens)),
                h("td", null, formatReferenceEstimate(row)),
              ))),
            ),
          ),
        ),
        h("article", { className: "plugin-statistics-card" },
          h("div", { className: "plugin-statistics-section-header" },
            h("strong", null, text("recent.title", "Recent physical attempts")),
            h(Badge, { variant: "outline" }, `${Math.min(filteredAttempts.length, MAX_RECENT_ATTEMPTS)} / ${filteredAttempts.length}`),
          ),
          filteredAttempts.length === 0 ? h(EmptyState, {
            title: text("recent.empty", "No physical attempt records"),
          }) : h("div", { className: "plugin-statistics-table-wrap" },
            h("table", { className: "plugin-statistics-table" },
              h("thead", null, h("tr", null,
                h("th", null, text("recent.time", "Time")),
                h("th", null, text("recent.model", "Provider / model")),
                h("th", null, text("recent.consumer", "Consumer")),
                h("th", null, text("recent.status", "Status")),
                h("th", null, text("recent.tokens", "Tokens")),
                h("th", null, text("recent.cost", "Reference estimate")),
                h("th", null, text("recent.error", "Error")),
              )),
              h("tbody", null, ...filteredAttempts.slice(0, MAX_RECENT_ATTEMPTS).map((attempt) => h("tr", { key: attempt.attemptId },
                h("td", null, formatTimestamp(attempt.terminalAtMs)),
                h("td", null, `${attempt.providerId} / ${attempt.modelId}`),
                h("td", null, attempt.consumerId),
                h("td", {
                  className: "plugin-statistics-status",
                  "data-status": attempt.status,
                }, text(`status.${attempt.status}`, attempt.status)),
                h("td", null, attempt.totalTokens === null ? "—" : formatInteger(attempt.totalTokens)),
                h("td", null, formatReferenceEstimate(attempt)),
                h("td", null, attempt.errorCategory
                  ? `${attempt.errorCategory}${attempt.errorCode ? ` / ${attempt.errorCode}` : ""}`
                  : "—"),
              ))),
            ),
          ),
        ),
        h("article", { className: "plugin-statistics-card" },
          h("div", { className: "plugin-statistics-section-header" },
            h("strong", null, text("accounts.title", "Account usage snapshots")),
            h(Badge, { variant: "outline" }, formatInteger(dashboard.accounts.length)),
          ),
          dashboard.accounts.length === 0 ? h(EmptyState, {
            title: text("accounts.empty", "No account usage snapshots"),
          }) : h("div", { className: "plugin-statistics-table-wrap" },
            h("table", { className: "plugin-statistics-table" },
              h("thead", null, h("tr", null,
                h("th", null, text("accounts.time", "Observed")),
                h("th", null, text("accounts.provider", "Provider")),
                h("th", null, text("accounts.connection", "Connection")),
                h("th", null, text("accounts.kind", "Snapshot kind")),
              )),
              h("tbody", null, ...dashboard.accounts.map((account) => h("tr", {
                key: `${account.providerId}/${account.connectionId}/${account.snapshotKind}`,
              },
                h("td", null, formatTimestamp(account.observedAtMs)),
                h("td", null, account.providerId),
                h("td", null, account.connectionId),
                h("td", null, account.snapshotKind),
              ))),
            ),
          ),
        ),
        h("p", { className: "plugin-statistics-privacy" },
          text("privacy", "Vulcan persists Token facts only. Reference prices and estimates remain plugin-owned and are recomputed for each dashboard query."),
        ),
      ) : null,
    );
  }

  return UsageStatisticsPanel;
}
