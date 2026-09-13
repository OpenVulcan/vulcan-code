import { pluginAssets } from "virtual:ui-audit-plugin-assets";

// AuditPluginScenario selects empty, populated or partial-failure synthetic responses.
// AuditPluginScenario 选择空数据、有数据或部分失败的合成响应。
export type AuditPluginScenario = "empty" | "invalid-default" | "long-list";
// prices are exact decimal strings, including a genuine free model and unknown prices represented separately by null.
// prices 是精确十进制字符串，包含真实免费模型；未知价格另用 null 表示。
const prices = { inputPerMillion: "1.25", outputPerMillion: "5", cacheReadPerMillion: "0.125", cacheWritePerMillion: "1.5625" };
const zeroPrices = { inputPerMillion: "0", outputPerMillion: "0", cacheReadPerMillion: "0", cacheWritePerMillion: "0" };
// modelRows provides more than one complete pricing page and models both with and without recorded use.
// modelRows 提供超过一整页的定价条目，以及有用量与无用量模型。
const modelRows = Array.from({ length: 42 }, (_, index) => ({
  modelId: `audit-model-${String(index + 1).padStart(2, "0")}`,
  attempts: index < 36 ? 1 : 0,
  source: index % 7 === 0 ? "unpriced" : index % 5 === 0 ? "override" : "catalog",
  prices: index % 7 === 0 ? null : index % 4 === 0 ? zeroPrices : prices,
}));
// fixtureToday anchors synthetic dates to the current audit day without reading installed data.
// fixtureToday 将合成日期锚定当前验收日期，不读取安装数据。
const fixtureToday = new Date();
fixtureToday.setUTCHours(0, 0, 0, 0);

// dailyRows creates one complete schema-valid terminal usage group per used model.
// dailyRows 为每个已使用模型创建一个完整且符合 Schema 的终态用量分组。
function dailyRows() {
  return modelRows.filter((model) => model.attempts > 0).map((model, index) => ({
    calendarDate: new Date(fixtureToday.getTime() - (index % 14) * 86400000).toISOString().slice(0, 10),
    providerId: "audit-provider", serviceEntryId: "audit-service", consumerId: index % 2 ? "conversation" : "internal_inference",
    operation: "conversation_respond", modelId: model.modelId, attemptCount: 1, successfulAttemptCount: 1, reportedUsageAttemptCount: 1,
    inputTokens: 1200, uncachedInputTokens: 750, cacheReadTokens: 400, cacheWriteTokens: 50,
    outputTokens: 300, reasoningTokens: 100, totalTokens: 1500,
    estimatedCost: model.prices === null ? null : model.prices === zeroPrices ? "0" : "0.002565625",
    estimatedCurrency: "USD", pricingMatchStatus: model.source === "unpriced" ? "unmatched" : model.source === "override" ? "override_exact" : "catalog_exact",
    pricingRevision: "audit-v1", pricingSourceId: "audit-catalog", aggregateRevision: 1,
  }));
}

// stringArgument validates a declared optional string field; defaults follow the plugin input schema.
// stringArgument 校验声明为可选的字符串字段；默认值遵循插件输入 Schema。
function stringArgument(args: Record<string, unknown>, key: string, defaultValue: string): string {
  const value = args[key];
  if (value === undefined) return defaultValue;
  if (typeof value !== "string") throw new Error(`UI_AUDIT_INVALID_PLUGIN_ARGUMENT: ${key}`);
  return value;
}
// integerArgument rejects malformed pagination and range fields before fixture evaluation.
// integerArgument 在求值夹具前拒绝格式错误的分页和时间范围字段。
function integerArgument(args: Record<string, unknown>, key: string, defaultValue: number): number {
  const value = args[key];
  if (value === undefined) return defaultValue;
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0 || value > 10000000) throw new Error(`UI_AUDIT_INVALID_PLUGIN_ARGUMENT: ${key}`);
  return value;
}
// rangeArgument accepts only the four ranges exposed by both real plugin schemas.
// rangeArgument 仅接受两个真实插件 Schema 暴露的四种时间范围。
function rangeArgument(args: Record<string, unknown>): number {
  const days = integerArgument(args, "rangeDays", 30);
  if (![7, 30, 90, 365].includes(days)) throw new Error("UI_AUDIT_INVALID_PLUGIN_ARGUMENT: rangeDays");
  return days;
}

// dashboard reads one coherent synthetic range before deriving totals, chart points and paged detail rows.
// dashboard 先读取同一合成范围，再派生总计、图表点与分页详情行。
// Parameters: scenario controls density; args is the exact dashboard-get input. Returns: its full declared output.
// 参数：scenario 控制密度；args 是准确的 dashboard-get 输入。返回值：完整声明输出。
function dashboard(scenario: AuditPluginScenario, args: Record<string, unknown>) {
  const days = rangeArgument(args);
  const consumer = stringArgument(args, "consumerId", "");
  const query = stringArgument(args, "modelQuery", "").toLowerCase();
  const view = stringArgument(args, "view", "all");
  const sort = stringArgument(args, "sortOrder", "newest");
  if (!["all", "overview", "details", "maintenance"].includes(view) || !["newest", "oldest"].includes(sort)) throw new Error("UI_AUDIT_INVALID_PLUGIN_ARGUMENT: view/sortOrder");
  const rows = (scenario === "empty" ? [] : dailyRows()).filter((row) =>
    (!consumer || row.consumerId === consumer) && row.modelId.toLowerCase().includes(query)
    && Date.parse(row.calendarDate) >= fixtureToday.getTime() - (days - 1) * 86400000)
    .sort((a, b) => (sort === "oldest" ? 1 : -1) * a.calendarDate.localeCompare(b.calendarDate) || a.modelId.localeCompare(b.modelId));
  // sumAtoms sums exact nine-place amounts before formatting; no floating point money enters the response.
  // sumAtoms 先汇总精确九位小数金额再格式化；响应不引入浮点金额。
  const sumAtoms = rows.reduce((sum, row) => sum + (row.estimatedCost === null || row.estimatedCost === "0" ? 0n : 2565625n), 0n);
  const amount = `${sumAtoms / 1000000000n}.${String(sumAtoms % 1000000000n).padStart(9, "0")}`;
  const unpriced = rows.filter((row) => row.estimatedCost === null).length;
  const dates = [...new Set(rows.map((row) => row.calendarDate))].sort();
  const heatmap = dates.map((date) => {
    const count = rows.filter((row) => row.calendarDate === date).length;
    return { date, value: count * 1500, requests: count, successes: count, topProviderName: "隔离供应商", topModelName: "合成模型" };
  });
  const attempts = rows.map((row) => ({ attemptId: `audit-attempt-${row.modelId}`, consumerId: row.consumerId, operation: row.operation,
    providerId: row.providerId, serviceEntryId: row.serviceEntryId, modelId: row.modelId, transport: "http", httpStatus: 200,
    websocketCloseCode: null, errorCategory: null, errorCode: null, status: "succeeded", usageAvailability: "reported",
    inputTokens: row.inputTokens, uncachedInputTokens: row.uncachedInputTokens, cacheReadTokens: row.cacheReadTokens,
    cacheWriteTokens: row.cacheWriteTokens, outputTokens: row.outputTokens, reasoningTokens: row.reasoningTokens,
    audioInputTokens: null, audioOutputTokens: null, totalTokens: row.totalTokens, billingDetails: [],
    estimatedCost: row.estimatedCost, estimatedCurrency: row.estimatedCurrency, pricingMatchStatus: row.pricingMatchStatus,
    pricingRevision: row.pricingRevision, pricingSourceId: row.pricingSourceId, terminalAtMs: Date.parse(row.calendarDate) + 3600000 }));
  return { view, generatedAtMs: fixtureToday.getTime(), schemaRevision: 1, aggregateRevision: 1, healthStatus: "ready",
    dailyCount: rows.length, recentCount: attempts.length, consumerIds: scenario === "empty" ? [] : ["conversation", "internal_inference"],
    summary: { costs: rows.length ? [{ currency: "USD", amount: unpriced === rows.length ? null : amount,
      pricedRows: rows.length - unpriced, incompleteRows: 0, unpricedRows: unpriced }] : [],
      attempts: rows.length, successes: rows.length, reported: rows.length, totalTokens: rows.length * 1500,
      inputTokens: rows.length * 1200, outputTokens: rows.length * 300, cacheReadTokens: rows.length * 400, cacheWriteTokens: rows.length * 50 },
    pricingCatalog: { catalogId: "audit-catalog", revision: "audit-v1", currency: "USD",
      entryCount: modelRows.filter((row) => row.source === "catalog").length, overrideCount: modelRows.filter((row) => row.source === "override").length,
      matchingStrategy: "exact_canonical_model_id" },
    daily: view === "details" || view === "all" ? rows.slice(integerArgument(args, "dailyOffset", 0), integerArgument(args, "dailyOffset", 0) + 100) : [],
    heatmap, series: heatmap.map((cell) => ({ date: cell.date, seriesKey: "audit-provider", label: "隔离供应商", value: cell.value })),
    recentAttempts: view === "details" || view === "all" ? attempts.slice(integerArgument(args, "recentOffset", 0), integerArgument(args, "recentOffset", 0) + 50) : [], accounts: [] };
}

// auditPluginCatalog returns detached manifest projections for real plugin navigation and host mounting.
// auditPluginCatalog 返回独立的清单投影，供真实插件导航与宿主挂载使用。
export function auditPluginCatalog() { return Object.values(pluginAssets).map((asset) => structuredClone(asset.summary)); }

// auditPluginModule admits one exact manifest mount and returns its current source without executing Lua or backend code.
// auditPluginModule 准入一个准确清单挂载点并返回当前源码，不执行 Lua 或后端代码。
export function auditPluginModule(pluginId: string, mountId: string) {
  const asset = pluginAssets[pluginId];
  if (!asset || asset.module.mountId !== mountId) throw new Error("UI_AUDIT_UNIMPLEMENTED_PLUGIN_MOUNT");
  return structuredClone(asset.module);
}

// auditPluginRead implements only explicitly admitted local synthetic reads, rejecting all writes and network actions.
// auditPluginRead 仅实现明确准入的本地合成读取，拒绝所有写入和联网动作。
// Parameters: pluginId/actionId identify the exact contract; args and scenario select synthetic results. Returns: a full output object.
// 参数：pluginId/actionId 标识准确契约；args 与 scenario 选择合成结果。返回值：完整输出对象。
export function auditPluginRead(pluginId: string, actionId: string, args: Record<string, unknown>, scenario: AuditPluginScenario): unknown {
  if (pluginId === "vulcan-usage-statistics") {
    if (actionId === "dashboard-get") return dashboard(scenario, args);
    if (actionId === "settings-get") return {
      modelOverrides: Object.fromEntries(modelRows.filter((row) => row.source === "override").map((row) => [row.modelId, row.prices])),
      catalogModels: modelRows.filter((row) => row.source === "catalog").map((row) => ({ modelId: row.modelId, prices: row.prices })),
    };
    if (actionId === "pricing-get") {
      const search = stringArgument(args, "search", "").toLowerCase();
      const filter = stringArgument(args, "filter", "all");
      const offset = integerArgument(args, "offset", 0);
      if (!["all", "used", "unpriced", "override"].includes(filter)) throw new Error("UI_AUDIT_INVALID_PLUGIN_ARGUMENT: filter");
      const models = modelRows.map((row) => ({ ...row, attempts: scenario === "empty" ? 0 : row.attempts }))
        .filter((row) => row.modelId.toLowerCase().includes(search) && (filter === "all" || filter === "used" && row.attempts > 0 || filter === row.source));
      return { totalCount: models.length, currency: "USD", models: models.slice(offset, offset + 25) };
    }
  }
  if (pluginId === "context7" && ["bootstrap-get", "usage-get"].includes(actionId)) {
    const rangeDays = rangeArgument(args);
    const credential = { configured: false, mode: "anonymous", credentialGeneration: 0, updatedAtMs: 0, validationStatus: "unknown", validatedAtMs: null };
    const requests = scenario === "empty" ? 0 : 12;
    const usage = { revision: 1, rangeDays, credential, localStats: { requests, successes: requests, failures: 0,
      averageDurationMs: requests ? 24.5 : 0, anonymousRequests: requests, apiKeyRequests: 0, resolveRequests: requests / 2, queryRequests: requests / 2 } };
    if (actionId === "usage-get") {
      if (scenario === "invalid-default") throw new Error("UI_AUDIT_SYNTHETIC_USAGE_FAILURE");
      return usage;
    }
    return { settings: { defaultFastMode: true, requestTimeoutMs: 30000, retentionDays: 90 }, credential,
      usage: scenario === "invalid-default" ? null : usage,
      usageError: scenario === "invalid-default" ? "隔离合成用量读取失败；未访问存储或网络。" : null };
  }
  throw new Error(`UI_AUDIT_UNIMPLEMENTED_PLUGIN_ACTION: ${pluginId}/${actionId}`);
}
