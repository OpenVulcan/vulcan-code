// ALL_CONSUMERS identifies the unfiltered system statistics projection.
// ALL_CONSUMERS 标识未筛选的系统统计投影。
const ALL_CONSUMERS = "__all_consumers__";

// MAX_DAILY_ROWS bounds the detailed aggregate table rendered in one panel pass.
// MAX_DAILY_ROWS 限制单次面板渲染的每日聚合明细数量。
const MAX_DAILY_ROWS = 100;

// MAX_RECENT_ATTEMPTS bounds the physical attempt evidence rendered in one panel pass.
// MAX_RECENT_ATTEMPTS 限制单次面板渲染的物理请求证据数量。
const MAX_RECENT_ATTEMPTS = 50;

// PRICING_PAGE_SIZE is the pricing-get page length; request offsets and pagination both derive from it.
// PRICING_PAGE_SIZE 是 pricing-get 的分页长度；请求偏移与分页导航均由其派生。
const PRICING_PAGE_SIZE = 25;

// DETAIL_TABLE_MIN_WIDTH keeps the dense detail tables scrollable instead of crushing their fixed columns.
// DETAIL_TABLE_MIN_WIDTH 让密集明细表格横向滚动，而不是挤压其固定列宽。
const DETAIL_TABLE_MIN_WIDTH = 1040;

// Resolve a shortcut using local calendar days, except the exact trailing 24-hour shortcut.
// 按本地自然日计算快捷范围，最近 24 小时快捷项除外。
// days is 0, 1, 7, 14 or 30; now is Unix milliseconds; returns the committed query bounds.
// days 为 0、1、7、14 或 30；now 为 Unix 毫秒；返回查询边界。
export function presetTimeRange(days, now = Date.now()) {
  // Calendar arithmetic preserves local midnight across daylight-saving transitions.
  // 自然日运算在夏令时切换时仍保留本地零点。
  const start = new Date(now);
  if (days === 1) start.setTime(now - 86400000);
  else {
    start.setDate(start.getDate() - Math.max(0, days - 1));
    start.setHours(0, 0, 0, 0);
  }
  return { startAtMs: start.getTime(), endAtMs: days === 0 ? null : now };
}

// CHART_COLORS is the inherited chart palette, handed to the series of one chart in order.
// CHART_COLORS 是继承的图表调色板，按顺序分给同一图表的各序列。
const CHART_COLORS = ["var(--chart-1)", "var(--chart-2)", "var(--chart-3)", "var(--chart-4)", "var(--chart-5)"];
// OTHER_SERIES_COLOR marks the folded remainder, which is never a model of its own.
// OTHER_SERIES_COLOR 标记折叠的剩余部分，它本身不是某个模型。
const OTHER_SERIES_COLOR = "var(--muted-foreground)";

// LATENCY_GRADES_MS holds the inclusive fast and medium upper bounds, in milliseconds, behind the latency grade;
// anything above the medium bound is slow.
// LATENCY_GRADES_MS 保存耗时分级的快速与中等上限（含，毫秒）；超过中等上限即为慢。
const LATENCY_GRADES_MS = {
  firstToken: [1500, 4000],
  request: [1000, 3000],
};

// CHART_RANGE_DAYS keeps the restored daily trend aligned with the runtime projection window.
// CHART_RANGE_DAYS 让恢复的每日趋势与运行时投影窗口保持一致。

// TOOLTIP_DELAY_MS keeps chart details intentional instead of appearing during pointer transit.
// TOOLTIP_DELAY_MS 让图表详情只在明确悬停后出现，避免指针经过时立即弹出。
const TOOLTIP_DELAY_MS = 500;

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

// Format exact non-negative decimals without converting the stored value to floating point.
// 格式化精确非负十进制数，不把存储值转换为浮点数。
// value is an exact decimal string and locale selects separators; returns compact display text.
// value 是精确十进制字符串，locale 选择分隔符；返回紧凑展示文本。
export function formatMoney(value, locale = "en") {
  if (typeof value !== "string" || !DECIMAL_PRICE_PATTERN.test(value)) return "-";
  // parts preserve every source digit, including values above Number.MAX_SAFE_INTEGER.
  // parts 保留全部来源数字，包括超过 Number.MAX_SAFE_INTEGER 的数值。
  const parts = value.split(".");
  // fraction is explicitly optional in the accepted decimal grammar.
  // fraction 在接受的十进制语法中明确可选。
  const fraction = parts[1] ?? "";
  // The decimal separator also applies to zero and values below display precision.
  // 小数分隔符同样适用于零值和低于展示精度的数值。
  const decimalSeparator = new Intl.NumberFormat(locale).formatToParts(1.1).find((part) => part.type === "decimal").value;
  if (BigInt(parts[0]) === 0n && !/[1-9]/.test(fraction)) return `0${decimalSeparator}00`;
  if (BigInt(parts[0]) === 0n && fraction.slice(0, 4).padEnd(4, "0") === "0000") return `<0${decimalSeparator}0001`;
  // places keeps small amounts useful while limiting ordinary amounts to cents.
  // places 保留小金额的有效信息，并将普通金额限制为分。
  let places = BigInt(parts[0]) === 0n ? 4 : 2;
  // scaled rounds integer digits only; the exact source is never changed.
  // scaled 仅对整数数字进行展示舍入；精确来源不会改变。
  let scaled = BigInt(parts[0] + fraction.slice(0, places).padEnd(places, "0"));
  if (Number(fraction[places] ?? "0") >= 5) scaled += 1n;
  if (places === 4 && scaled >= 10000n) { scaled = 100n; places = 2; }
  // digits provides the rounded integer and fractional portions.
  // digits 提供舍入后的整数与小数部分。
  const digits = scaled.toString().padStart(places + 1, "0");
  // integer is formatted as BigInt to preserve arbitrarily large exact values.
  // integer 使用 BigInt 格式化，以保留任意大精确值。
  const integer = new Intl.NumberFormat(locale).format(BigInt(digits.slice(0, -places)));
  // tail removes insignificant zeros only for sub-unit amounts.
  // tail 仅在小于一的金额中移除无意义尾零。
  const tail = places === 4 ? digits.slice(-places).replace(/0+$/, "") : digits.slice(-places);
  // rounded reports only actual precision loss.
  // rounded 仅表示确实存在精度舍弃。
  const rounded = /[1-9]/.test(fraction.slice(places));
  return `${rounded ? "≈" : ""}${integer}${tail ? decimalSeparator + tail : ""}`;
}

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
  // components is the host SDK surface: the page kit owns page structure; controls, dialogs and charts fill it.
  // components 是宿主 SDK 接口：页面套件负责页面结构，控件、对话框与图表填充其中。
  const {
    PageBody, PageToolbar, PageTabs, PageTabPanel, PageSection, PageHint, PageNotice, PageEmptyState, PageFacts, PageTable,
    MetricCard, MetricGrid, Badge, Button, Input, Checkbox, SelectControl, StatusPill, HelpTooltip, StreamPlaceholder,
    Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter,
    ActivityHeatmap, ActivityHeatmapLegend, TimeSeriesLineChart, ChartLegend,
  } = sdk.components;
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

  // Format measured display numbers without changing their stored values.
  // 格式化实测展示数值，不改变存储值。
  // value is a finite measurement; places is its displayed precision; returns localized text.
  // value 为有限测量值；places 为展示精度；返回本地化文本。
  function formatDecimal(value, places) {
    return new Intl.NumberFormat(sdk.locale, {
      useGrouping: false, minimumFractionDigits: places, maximumFractionDigits: places,
    }).format(value);
  }

  // Render percentage measurements with the host locale's decimal and percent spacing.
  // 使用宿主语言的小数及百分号间距渲染百分比测量值。
  // value is a measured percentage on the 0–100 scale; returns one-decimal localized text.
  // value 为 0–100 尺度的测量百分比；返回一位小数的本地化文本。
  function formatPercent(value) {
    return new Intl.NumberFormat(sdk.locale, {
      style: "percent", minimumFractionDigits: 1, maximumFractionDigits: 1,
    }).format(value / 100);
  }

  // Localize interval units using the host locale rather than English abbreviations.
  // 按宿主语言本地化时间单位，不使用固定英文缩写。
  // value is a measured interval; unit is second or minute; places controls precision.
  // value 为实测区间；unit 为秒或分钟；places 控制精度。
  // Returns a compact localized duration component.
  // 返回紧凑的本地化时长片段。
  function formatInterval(value, unit, places) {
    return new Intl.NumberFormat(sdk.locale, {
      style: "unit", unit, unitDisplay: "narrow", useGrouping: false,
      minimumFractionDigits: places, maximumFractionDigits: places,
    }).format(value);
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
      if (record.estimatedCost === null || record.estimatedCost === undefined) return "-";
      return currencyAmount(formatMoney(record.estimatedCost, sdk.locale), record.estimatedCurrency);
    }

  // Render dollar amounts with their symbol while retaining exact decimal text and approximation marks.
  // 使用美元符号渲染金额，同时保留精确小数文本与近似标记。
  // value is formatted or exact decimal text; currency is the declared code; returns display text.
  // value 为格式化或精确小数文本；currency 为声明币种；返回展示文本。
  function currencyAmount(value, currency) {
    if (currency === "USD") return value.startsWith("≈") ? `≈$${value.slice(1)}` : `$${value}`;
    return currency ? `${value} ${currency}` : value;
  }

  // Identify exact decimal zero without rounding small positive costs away.
  // 识别精确十进制零，不将较小正费用舍入为零。
  // value is nullable decimal text; returns true only for explicitly recorded zero.
  // value 为可空小数文本；仅对明确记录的零返回 true。
  function isZeroCost(value) { return typeof value === "string" && /^0(?:\.0+)?$/.test(value); }

  // Format local editable date/time text without applying a UTC offset to the displayed clock.
  // 格式化本地可编辑日期时间，不对显示时钟施加 UTC 偏移。
  // milliseconds is a Unix timestamp; returns the native datetime-local input format.
  // milliseconds 为 Unix 时间戳；返回原生 datetime-local 输入格式。
  function localDateTimeValue(milliseconds) {
    // Read the local wall clock from the exact timestamp.
    // 从精确时间戳读取本地时钟。
    const date = new Date(milliseconds);
    return `${formatLocalDateKey(date)}T${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
  }

  // Display the committed range with a live end marker when requested.
  // 展示已提交范围，并按需显示实时结束标记。
  // range has startAtMs and nullable endAtMs; returns a compact local range label.
  // range 包含 startAtMs 与可空 endAtMs；返回紧凑本地范围标签。
  function timeRangeLabel(range) {
    return `${localDateTimeValue(range.startAtMs).replace("T", " ")} — ${range.endAtMs === null ? text("range.now", "Now") : localDateTimeValue(range.endAtMs).replace("T", " ")}`;
  }

  // Stage a date/time interval and commit only after explicit confirmation.
  // 暂存日期时间区间，仅在明确确认后提交。
  // value owns the active bounds; onChange receives validated bounds; returns the range picker.
  // value 持有当前边界；onChange 接收有效边界；返回时间范围选择器。
  function TimeRangeControl({ value, onChange }) {
    // Keep dialog visibility and draft fields isolated from the active query.
    // 将弹窗可见性与草稿字段和当前查询隔离。
    const [open, setOpen] = React.useState(false);
    // Start and end use local input values until confirmation converts them into timestamps.
    // 开始与结束使用本地输入值，直到确认时转为时间戳。
    const [start, setStart] = React.useState("");
    const [end, setEnd] = React.useState("");
    // Preserve exact shortcut timestamps until a date/time field is edited manually.
    // 在手动编辑日期时间字段前保留快捷项的精确时间戳。
    const [exactBounds, setExactBounds] = React.useState(null);
    // A following end is captured afresh for each explicit statistics query.
    // 跟随结束时间在每次显式统计查询时重新取值。
    const [followNow, setFollowNow] = React.useState(true);
    // Validation feedback belongs to the draft, never to already displayed statistics.
    // 校验反馈属于草稿，不属于已展示的统计。
    const [rangeError, setRangeError] = React.useState("");
    return h(React.Fragment, null,
      h(Button, { type: "button", size: "sm", variant: "outline", "aria-label": text("range.label", "Time range"), onClick: () => {
        setStart(localDateTimeValue(value.startAtMs)); setEnd(localDateTimeValue(value.endAtMs ?? Date.now()));
        setExactBounds({ startAtMs: value.startAtMs, endAtMs: value.endAtMs ?? Date.now() });
        setFollowNow(value.endAtMs === null); setRangeError(""); setOpen(true);
      } }, timeRangeLabel(value)),
      h(Dialog, { open, onOpenChange: setOpen },
        h(DialogContent, { size: "md" },
          h(DialogHeader, null,
            h(DialogTitle, null, text("range.label", "Time range")),
            h(DialogDescription, null, text("range.description", "Choose local dates and times."))),
          h("div", { className: "plugin-statistics-range-presets" }, ...[0, 1, 7, 14, 30].map((days) => h(Button, {
            key: days, type: "button", size: "sm", variant: "outline", onClick: () => {
              // Only Today follows now; other shortcuts capture a fixed endpoint when selected.
              // 仅当天跟随当前时刻；其他快捷项在选择时固定结束时间。
              const now = Date.now();
              const bounds = presetTimeRange(days, now);
              setExactBounds({ startAtMs: bounds.startAtMs, endAtMs: now });
              setStart(localDateTimeValue(bounds.startAtMs)); setEnd(localDateTimeValue(now)); setFollowNow(days === 0); setRangeError("");
            },
          }, days === 0 ? text("range.today", "Today") : days === 1 ? text("range.last24Hours", "1 day") : interpolate(text("range.lastDays", "{{days}} days"), { days })))),
          h("div", { className: "plugin-statistics-range-fields" },
            h("label", null, h("span", null, text("range.start", "Start time")), h(Input, { type: "datetime-local", value: start, onChange: (event) => { setStart(event.target.value); setExactBounds(null); } })),
            h("label", null, h("span", null, text("range.end", "End time")), h(Input, { type: "datetime-local", value: end, disabled: followNow, onChange: (event) => { setEnd(event.target.value); setExactBounds(null); } })),
          ),
          h("label", { className: "plugin-statistics-range-follow" }, h(Checkbox, { checked: followNow, onCheckedChange: (checked) => setFollowNow(checked === true) }), text("range.followNow", "End time follows now")),
          rangeError ? h(PageNotice, { tone: "error" }, rangeError) : null,
          h(DialogFooter, null,
            h(Button, { type: "button", variant: "ghost", onClick: () => setOpen(false) }, text("clear.cancel", "Cancel")),
            h(Button, { type: "button", onClick: () => {
              // Reject missing or reversed times before issuing a query.
              // 发起查询前拒绝缺失或倒置的时间。
              const startAtMs = exactBounds === null ? new Date(start).getTime() : exactBounds.startAtMs;
              const endAtMs = followNow ? Date.now() : exactBounds === null ? new Date(end).getTime() : exactBounds.endAtMs;
              if (!Number.isFinite(startAtMs) || !Number.isFinite(endAtMs) || startAtMs <= 0 || endAtMs <= startAtMs) {
                setRangeError(text("range.invalid", "End time must be later than start time.")); return;
              }
              onChange({ startAtMs, endAtMs: followNow ? null : endAtMs }); setOpen(false);
            } }, text("range.confirm", "Confirm")),
          ),
        ),
      ),
    );
  }

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

  // colorForSeries gives the series of one chart distinct palette colors in order; a hash of the key would let two
  // models of the same chart share a color.
  // colorForSeries 按顺序为同一图表的序列分配互不相同的调色板颜色；按键哈希会让同一图表的两个模型撞色。
  function colorForSeries(id, position) {
    return id === "other" ? OTHER_SERIES_COLOR : CHART_COLORS[position % CHART_COLORS.length];
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

  // Return ordered local calendar keys covering the supplied timestamp bounds.
  // 返回覆盖给定时间戳边界的有序本地日期键。
  function localCalendarDates(range) {
    // Normalize the inclusive end to local midnight for calendar arithmetic.
    // 将闭区间终点规范化到本地午夜以进行日期运算。
    const today = new Date(range.endAtMs ?? Date.now());
    today.setHours(0, 0, 0, 0);
    // start is the first requested local calendar date.
    // start 是请求的首个本地自然日。
    const start = new Date(range.startAtMs);
    start.setHours(0, 0, 0, 0);
    // dates accumulates every local date exactly once.
    // dates 恰好一次累积每个本地日期。
    const dates = [];
    for (const cursor = new Date(start); cursor <= today; cursor.setDate(cursor.getDate() + 1)) {
      dates.push(formatLocalDateKey(cursor));
    }
    return dates;
  }

  // buildHeatmapCells fills the shared calendar grid and derives five non-zero intensity bands.
  // buildHeatmapCells 填充共享日历点阵并派生五级非零强度区间。
  function buildHeatmapCells(rows, range) {
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
    return localCalendarDates(range).map((date) => {
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
      // requests is the exact physical request count used by the tooltip.
      // requests 是提示中使用的精确真实请求数。
      const requests = numeric(row?.requests);
      // successes is the exact successful request count used by the tooltip.
      // successes 是提示中使用的精确成功请求数。
      const successes = numeric(row?.successes);
      // A day without requests has no observed success rate, rather than a failed zero-percent rate.
      // 无请求的日期没有观测成功率，不能显示为全部失败的零百分比。
      const successRate = requests === 0 ? null : successes * 100 / requests;
      // topModel is the provider/model pair returned by the authoritative daily projection.
      // topModel 是权威每日投影返回的供应商/模型组合。
      const topModel = row?.topModelName
        ? `${row.topProviderName} / ${row.topModelName}`
        : text("heatmap.noModel", "No model");
      return {
        date,
        intensity,
        label: interpolate(
          text("heatmap.tooltip", "{{date}}: {{value}} tokens · {{requests}} requests · {{successRate}} success · {{model}}"),
          {
            date: new Intl.DateTimeFormat(sdk.locale, { month: "long", day: "numeric" }).format(new Date(`${date}T00:00:00`)),
            model: topModel,
            requests: formatInteger(requests),
            successRate: successRate === null ? "—" : formatPercent(successRate),
            value: formatInteger(value),
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

  // buildLineSeries aligns sparse model rows onto one selected-range local-date axis.
  // buildLineSeries 将稀疏模型行对齐到所选范围的本地自然日横轴。
  function buildLineSeries(rows, range) {
    // dates is the complete fixed chart x-axis.
    // dates 是完整的固定图表横轴。
    const dates = localCalendarDates(range);
    // groups accumulates exact values by stable series key and date.
    // groups 按稳定序列键与日期累积精确数值。
    const groups = new Map();
    for (const row of rows) {
      if (!groups.has(row.seriesKey)) {
        groups.set(row.seriesKey, { label: row.label, values: new Map() });
      }
      // group is the exact series accumulator for one stable model identity.
      // group 是一个稳定模型身份对应的精确序列累加器。
      const group = groups.get(row.seriesKey);
      group.values.set(row.date, numeric(row.value));
    }
    // position counts model series only, so the folded remainder never consumes a palette color.
    // position 只计模型序列，折叠的剩余部分不占用调色板颜色。
    let position = 0;
    return Array.from(groups.entries()).map(([id, group]) => ({
      id,
      label: id === "other" ? text("other", "Other models") : group.label,
      color: colorForSeries(id, id === "other" ? 0 : position++),
      points: dates.map((date) => ({ key: date, value: group.values.get(date) ?? 0 })),
    }));
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

  // Exclude embedding and reranking operations, which return results without token generation timing.
  // 排除返回完整结果、不具有 Token 生成计时的向量与重排序操作。
  // record supplies the schema-required operation; returns whether token timing belongs in the presentation.
  // record 提供 Schema 必需的 operation；返回展示是否应包含 Token 计时。
  function usesTokenGenerationTiming(record) {
    return record.operation !== "embedding_create" && record.operation !== "rerank_documents";
  }

  // Render exact usage dimensions and the cache-read share of reported input.
  // 渲染精确用量维度，以及缓存读取占已报告输入的比例。
  // record is a daily or attempt projection; returns two icon rows and exact hover details.
  // record 是每日或请求投影；返回两行图标数值及精确悬停详情。
  function tokenDetails(record, aggregate = false) {
    // The operation contract governs both visible timing and tooltip details.
    // 操作契约同时决定可见计时与提示详情。
    const tokenTiming = usesTokenGenerationTiming(record);
    // Only reported dimensions belong in the tooltip; zero remains a reported value.
    // 提示仅展示已报告的维度；零仍然属于已报告数值。
    const dimensions = ["inputTokens", "uncachedInputTokens", "outputTokens", "cacheReadTokens", "cacheWriteTokens", "reasoningTokens", "totalTokens"].filter((key) => record[key] !== null);
    if (dimensions.length === 0 && record.requestDurationMs === null) return null;
    return h("div", { className: "plugin-statistics-token-cell" },
      h("div", { className: "plugin-statistics-token-lines" },
        h("div", null, tokenValue("uncachedInputTokens", record.uncachedInputTokens, "input"), tokenValue("outputTokens", record.outputTokens, "output")),
        h("div", null, tokenValue("cacheReadTokens", record.cacheReadTokens, "cache"),
          record.cacheWriteTokens !== null && record.cacheWriteTokens > 0 ? tokenValue("cacheWriteTokens", record.cacheWriteTokens, "write") : null),
      ),
      h(HelpTooltip, { ariaLabel: text("tokens.details", "Token details"), content: h("div", { className: "plugin-statistics-detail-tooltip" },
        h("strong", null, text("tokens.details", "Token details")),
        facts(dimensions.map((key) => [text(`tokens.${key}`, key), formatInteger(record[key])])),
        h("div", { className: "plugin-statistics-cost-total" }, facts([
          [text(aggregate ? "timing.averageRequest" : "timing.request", "Request duration"), record.requestDurationMs === null ? null : formatInterval(record.requestDurationMs / 1000, "second", 3)],
          [text(aggregate ? "timing.averageFirstToken" : "timing.firstToken", "First-token latency"), !tokenTiming || record.firstTokenLatencyMs === null ? null : formatInterval(record.firstTokenLatencyMs / 1000, "second", 3)],
          [text(aggregate ? "timing.totalOutput" : "timing.output", "Output duration"), !tokenTiming || record.outputDurationMs === null ? null : formatInterval(record.outputDurationMs / 1000, "second", 3)],
          [text("timing.speed", "Output speed"), !tokenTiming || record.outputTokensPerSecond === null ? null : interpolate(text("timing.tokensPerSecond", "{{value}} tokens/s"), { value: formatDecimal(record.outputTokensPerSecond, 2) })],
          [text("timing.samples", "Measured requests"), record.timedAttemptCount > 0 ? formatInteger(record.timedAttemptCount) : null],
        ])),
        tokenTiming ? h("p", null, text("timing.formula", "Output speed uses paired output tokens and completion minus first-token duration; missing timing is not estimated.")) : null,
        h("p", null, text("tokens.cacheSeparate", "Cache counts are already included in reported input; do not add them again.")),
      ) }),
    );
  }

  // Format measured milliseconds for compact latency rows; return seconds or minutes plus seconds.
  // 格式化实测毫秒数用于紧凑延迟行；返回秒或分钟加秒。
  // milliseconds is a known non-negative interval, never a substituted missing measurement.
  // milliseconds 是已知非负区间，绝不用于代替缺失测量。
  function latencyDuration(milliseconds) {
    // Preserve subsecond precision for short model output intervals.
    // 为较短的模型输出区间保留毫秒精度。
    const seconds = milliseconds / 1000;
    return seconds < 60 ? formatInterval(seconds, "second", seconds < 1 ? 3 : 2)
      : `${formatInterval(Math.floor(seconds / 60), "minute", 0)} ${formatInterval(seconds % 60, "second", 2)}`;
  }

  // Grade how fast a record answered: generation is judged by first-token latency, other operations by request
  // duration. record is a schema-validated projection; tokenTiming selects the measure.
  // Returns "fast", "medium" or "slow", or undefined when the judged measure was not recorded.
  // 评定一条记录的响应快慢：生成类按首 Token 延迟评定，其他操作按请求耗时评定。record 是通过 Schema 校验的投影；
  // tokenTiming 选择评定所用的测量值。返回 "fast"、"medium" 或 "slow"；所评定的测量值未记录时返回 undefined。
  function latencySpeed(record, tokenTiming) {
    const milliseconds = tokenTiming ? record.firstTokenLatencyMs : record.requestDurationMs;
    if (milliseconds === null || milliseconds === undefined) return undefined;
    const [fastUpTo, mediumUpTo] = tokenTiming ? LATENCY_GRADES_MS.firstToken : LATENCY_GRADES_MS.request;
    return milliseconds <= fastUpTo ? "fast" : milliseconds <= mediumUpTo ? "medium" : "slow";
  }

  // Render first-token latency and full request duration, with generation speed beside duration.
  // 渲染首 Token 延迟和完整请求耗时，在耗时旁展示生成速度。
  // record is a schema-validated attempt or daily projection; aggregate selects average labels.
  // record 是通过 Schema 校验的请求或每日投影；aggregate 选择平均值标签。
  // Returns operation-appropriate timing rows, or an explicit unrecorded state for historical data.
  // 返回适用于当前操作的计时行，历史数据缺失时返回明确的未记录状态。
  function latencyDetails(record, aggregate = false) {
    // Embedding and reranking retain request duration without a first-token placeholder.
    // 向量与重排序仅保留请求耗时，不显示首 Token 占位。
    const tokenTiming = usesTokenGenerationTiming(record);
    if (record.requestDurationMs === null && (!tokenTiming || record.firstTokenLatencyMs === null)) {
      return h("span", { className: "plugin-statistics-latency-unknown" }, text("timing.unrecorded", "Not recorded"));
    }
    return h("div", { className: "plugin-statistics-latency", "data-speed": latencySpeed(record, tokenTiming) },
      tokenTiming ? h("div", null,
        h("span", null, text(aggregate ? "timing.averageFirstShort" : "timing.firstShort", "First token")),
        h("span", null, record.firstTokenLatencyMs === null ? text("timing.unrecorded", "Not recorded") : latencyDuration(record.firstTokenLatencyMs))) : null,
      h("div", null,
        h("span", null, text(aggregate ? "timing.averageTotalShort" : "timing.totalShort", "Total")),
        h("span", null,
          record.requestDurationMs === null ? text("timing.unrecorded", "Not recorded") : latencyDuration(record.requestDurationMs),
          !tokenTiming || record.outputTokensPerSecond === null ? null : ` (${interpolate(text("timing.tokensPerSecond", "{{value}} tokens/s"), { value: formatDecimal(record.outputTokensPerSecond, 1) })})`)),
    );
  }

  // facts renders reported label/value pairs. rows contains nullable values; returns one definition list.
  // facts 渲染已报告的名称和值。rows 包含可为空的值；返回一个定义列表。
  function facts(rows) {
    return h("dl", { className: "plugin-statistics-detail-facts" }, ...rows.filter(([, value]) => value !== null && value !== "").map(([label, value], index) => h(React.Fragment, { key: index }, h("dt", null, label), h("dd", null, value))));
  }

  // tokenValue uses the actual uncached/input/output/cache facts; unknown counts stay unknown.
  // tokenValue 使用实际非缓存输入、输出及缓存事实；未知计数保持未知。
  // key supplies the localized label, value is nullable, kind chooses the SVG; returns one compact value.
  // key 提供本地化名称，value 可为空，kind 选择图标；返回紧凑数值。
  function tokenValue(key, value, kind) {
    if (value === null) return null;
    return h("span", { className: "plugin-statistics-token-value", "data-kind": kind, title: text(`tokens.${key}`, key) },
      h("svg", { width: 14, height: 14, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.6, "aria-hidden": true },
        kind === "input" ? h("path", { d: "M12 3v18m-7-7 7 7 7-7" })
          : kind === "output" ? h("path", { d: "M12 21V3m-7 7 7-7 7 7" })
            : h("path", { d: kind === "write" ? "M4 4h16v5H4zM6 9v11h12V9M12 11v6m-3-3 3 3 3-3" : "M4 4h16v5H4zM6 9v11h12V9M9 13h6" })),
      h("span", { "aria-label": text(`tokens.${key}`, key) }, formatInteger(value)));
  }

  // consumerLabel localizes built-in caller identities; arbitrary agent names retain their exact identity.
  // consumerLabel 本地化内置调用方身份；自定义智能体名称保留精确身份。
  // id is the persisted caller key; returns a natural-language consumer name.
  // id 是持久化调用方键；返回自然语言消费者名称。
  function consumerLabel(id) {
    if (id.startsWith("vulcan-code-vce:")) return text("consumer.codeContext", "Code context");
    return text(`consumer.name.${id}`, interpolate(text("consumer.custom", "Custom caller · {{id}}"), { id }));
  }

  // operationLabel translates protocol operation identifiers without changing the stored operation.
  // operationLabel 翻译协议操作标识，不改变存储的操作。
  // operation is the source key; returns localized prose or an explicitly named custom operation.
  // operation 是来源键；返回本地化说明或明确标注的自定义操作。
  function operationLabel(operation) {
    return text(`operation.${operation}`, interpolate(text("operation.custom", "Custom operation · {{id}}"), { id: operation }));
  }

  // modelIdentity renders the model and its provider as two lines. record supplies exact identities.
  // modelIdentity 将模型和供应商分两行显示。record 提供精确身份。
  // Returns a compact, wrapping model cell with the full values available on hover.
  // 返回紧凑可换行的模型单元格，悬停可读取完整值。
  function modelIdentity(record) {
    return h("div", { className: "plugin-statistics-model-identity" }, h("span", { title: record.modelId }, record.modelId), h("small", { title: record.providerId }, record.providerId));
  }

  // errorDetails exposes all persisted error facts; absent provider prose is explicitly identified.
  // errorDetails 展示已保存的全部错误事实；明确说明供应商原文缺失。
  // attempt is one physical request; returns an error tooltip or a placeholder for a successful request.
  // attempt 是一次真实请求；返回错误提示，成功请求保留占位标记。
  function errorDetails(attempt) {
    if (attempt.status === "succeeded" && !attempt.errorCategory && !attempt.errorCode) return "—";
    return h(HelpTooltip, { tone: "error", ariaLabel: text("error.details", "Error details"), content: h("div", { className: "plugin-statistics-detail-tooltip" },
      h("strong", null, text(`failure.${attempt.errorCategory}`, text("failure.unknown", "The request did not complete successfully."))),
      facts([
        [text("recent.status", "Status"), text(`status.${attempt.status}`, attempt.status)],
        [text("error.category", "Category"), attempt.errorCategory],
        [text("error.code", "Error code"), attempt.errorCode],
        ["HTTP", attempt.httpStatus],
        ["WebSocket", attempt.websocketCloseCode],
        [text("daily.operation", "Operation"), operationLabel(attempt.operation)],
        [text("error.requestId", "Request ID"), attempt.attemptId],
      ]), h("p", null, text("error.noOriginal", "The statistics record does not contain the original error message.")),
    ) });
  }

  // tokenOverview visualizes reported input/output only; cache dimensions never add to that denominator.
  // tokenOverview 仅可视化已报告输入与输出；缓存维度绝不加入该分母。
  // record is the full-range host summary; returns compact values and a labelled composition bar when both operands are known.
  // record 是全范围宿主汇总；返回紧凑数值，并仅在两个操作数已知时返回带名称的占比条。
  function tokenOverview(record) {
    // total combines disjoint input/output dimensions; null and zero cannot manufacture percentages.
    // total 合并互不重叠的输入与输出维度；空值与零值不能被编造成比例。
    const total = record.inputTokens !== null && record.outputTokens !== null ? record.inputTokens + record.outputTokens : null;
    const inputShare = total !== null && total > 0 ? record.inputTokens / total * 100 : null;
    const outputShare = inputShare === null ? null : 100 - inputShare;
    const cacheShare = record.inputTokens !== null && record.inputTokens > 0 && record.cacheReadTokens !== null
      ? formatPercent(record.cacheReadTokens / record.inputTokens * 100) : "-";
    return h(React.Fragment, null,
      h("div", { className: "plugin-statistics-token-values" },
        ...[["inputTokens", inputShare], ["outputTokens", outputShare]].map(([key, share]) => h("div", { key, "data-token-dimension": key },
          h("span", null, text(`tokens.${key}`, key)),
          h("strong", null, record[key] === null ? "-" : formatInteger(record[key])),
          h("span", { className: "plugin-statistics-token-share" }, share === null ? "-" : formatPercent(share)),
        )),
      ),
      inputShare === null ? h(PageHint, null, text("tokens.noComposition", "Input/output share is unavailable"))
        : h("div", { className: "plugin-statistics-token-bar", role: "img", "aria-label": interpolate(text("tokens.composition", "Input {{input}} · Output {{output}}"), {
          input: formatPercent(inputShare), output: formatPercent(outputShare),
        }) }, h("span", { style: { width: `${inputShare}%` } }), h("span", { style: { width: `${outputShare}%` } })),
      h(PageFacts, { className: "plugin-statistics-cache-facts", layout: "list", items: [
        { key: "cacheRead", label: text("tokens.cacheReadTokens", "Cache read"),
          value: `${record.cacheReadTokens === null ? "-" : formatInteger(record.cacheReadTokens)} · ${cacheShare}` },
        { key: "cacheWrite", label: text("tokens.cacheWriteTokens", "Cache write"),
          value: record.cacheWriteTokens === null ? "-" : formatInteger(record.cacheWriteTokens) },
      ] }),
      h(PageHint, null, text("tokens.cacheSeparate", "Cache read share uses reported input; cache counts are not added to input/output.")),
    );
  }

  // Keep short cost cells while exposing the exact estimate and missing-price reason.
  // 保持费用单元格简短，同时提供精确估算与价格缺失原因。
  // record is a priced runtime projection; returns the estimate and its exact reference-price breakdown.
  // record 是运行时定价投影；返回估算和精确参考价格明细。
  function estimateDetails(record) {
    // Exclude only explicitly zero dimensions; unknown costs retain their explanation.
    // 仅排除明确为零的费用维度；未知费用保留说明。
    const dimensions = record.pricingDetails === null ? [] : record.pricingDetails.dimensions.filter((dimension) => !isZeroCost(dimension.cost));
    return h("div", { className: "plugin-statistics-estimate-cell" },
      record.estimatedCost === null ? null : h("span", null, formatReferenceEstimate(record)),
      h(HelpTooltip, { ariaLabel: text("cost.details", "Reference cost details"), content: h("div", { className: "plugin-statistics-detail-tooltip" },
        h("strong", null, text("cost.details", "Reference cost details")),
        record.pricingDetails === null ? h("p", null, text("pricing.unpriced", "Unpriced"))
          : h(React.Fragment, null,
          dimensions.length === 0 ? h("p", null, text("cost.noNonzero", "No nonzero cost items")) : h("table", { className: "plugin-statistics-cost-table" },
            h("thead", null, h("tr", null,
              h("th", null, text("cost.item", "Item")),
              h("th", null, text("cost.component", "Estimated cost")),
              h("th", null, text("cost.unitPerMillion", "Price / 1M Token")),
            )),
            h("tbody", null, ...dimensions.map((dimension) => h("tr", { key: dimension.tokenField },
              h("td", null, text(`tokens.${dimension.tokenField}`, dimension.tokenField)),
              h("td", null, dimension.cost === null ? text("pricing.incompleteUsage", "Required usage is incomplete") : currencyAmount(dimension.cost, record.pricingDetails.currency)),
              h("td", null, currencyAmount(dimension.perMillion, record.pricingDetails.currency)),
            ))),
          ),
          facts([[text("pricing.source", "Price source"), text(record.pricingMatchStatus === "override_exact" || record.pricingSourceId?.startsWith("plugin.config.modelOverrides:") ? "pricing.override" : "pricing.default", "Reference price")]])),
        h("div", { className: "plugin-statistics-cost-total" }, facts([[text("cost.total", "Total reference estimate"), record.estimatedCost === null
          ? text(record.pricingMatchStatus === "unmatched" ? "pricing.unpriced" : "pricing.incompleteUsage", "Estimate unavailable")
          : currencyAmount(record.estimatedCost, record.estimatedCurrency)]])),
        h("p", null, text("cost.referenceOnly", "Calculated from recorded usage and current reference prices; this is not a provider bill.")),
      ) }),
    );
  }

  // Render server page navigation without claiming the loaded rows are the complete dataset.
  // 渲染服务端分页导航，不将已加载行冒充完整数据集。
  // offset, size and total describe the server page; setOffset changes it and busy prevents overlap.
  // offset、size 与 total 描述服务端分页；setOffset 切换分页，busy 防止重叠。
  // Returns the position hint and both page buttons for a PageSection footer.
  // 返回用于 PageSection 页脚的位置提示与前后翻页按钮。
  function pageControls(offset, size, total, setOffset, busy) {
    return h(React.Fragment, null,
      h(PageHint, null, `${Math.min(offset + 1, total)}–${Math.min(offset + size, total)} / ${formatInteger(total)}`),
      h(Button, { type: "button", size: "sm", variant: "outline", disabled: busy || offset === 0, onClick: () => setOffset(Math.max(0, offset - size)) }, text("page.previous", "Previous")),
      h(Button, { type: "button", size: "sm", variant: "outline", disabled: busy || offset + size >= total, onClick: () => setOffset(offset + size) }, text("page.next", "Next")),
    );
  }

  // UsageStatisticsPanel combines host-owned Token facts with an in-memory plugin price catalog.
  // UsageStatisticsPanel 将宿主持有的 Token 事实与插件内存价目表组合展示。
  function UsageStatisticsPanel() {
    // section selects one reachable task area without losing unsaved pricing drafts.
    // section 选择可达任务区域，同时保留尚未保存的定价草稿。
    const [section, setSection] = React.useState("overview");
    // dashboardView requests only projections used by the active task area; pricing reads its own directory.
    // dashboardView 仅请求当前任务区域使用的投影；定价读取独立目录。
    const dashboardView = section === "pricing" ? "overview" : section;
    // Apply one exact time interval to all usage queries; null end follows query time.
    // 为所有用量查询应用同一精确时间区间；空终点跟随查询时刻。
    const [timeRange, setTimeRange] = React.useState(() => ({ ...presetTimeRange(30), endAtMs: null }));
    // modelQuery and sortOrder bind explicit search and chronological ordering to the server.
    // modelQuery 与 sortOrder 将明确搜索和时间排序绑定到服务端。
    const [modelQuery, setModelQuery] = React.useState("");
    const [modelDraft, setModelDraft] = React.useState("");
    const [sortOrder, setSortOrder] = React.useState("newest");
    // dailyOffset and recentOffset select independent server pages.
    // dailyOffset 与 recentOffset 选择独立的服务端分页。
    const [dailyOffset, setDailyOffset] = React.useState(0);
    const [recentOffset, setRecentOffset] = React.useState(0);
    // Select a server-filtered model category; switching categories starts at its first page.
    // 选择服务端筛选的模型类别；切换类别时从该类别首页开始。
    const [recentModelType, setRecentModelType] = React.useState("llm");
    // providerFilter narrows the Provider tab to one exact Provider; empty selects all Providers.
    // providerFilter 将供应商分类限定为一个精确 Provider；空值表示全部 Provider。
    const [providerFilter, setProviderFilter] = React.useState("");
    // pricingSearch and pricingFilter narrow the complete declared price catalog.
    // pricingSearch 与 pricingFilter 筛选完整声明的定价目录。
    const [pricingSearch, setPricingSearch] = React.useState("");
    const [pricingFilter, setPricingFilter] = React.useState("all");
    const [pricingPage, setPricingPage] = React.useState(0);
    // pricingDirectory is a server page across all retained usage, catalog and override models.
    // pricingDirectory 是横跨全部保留用量、目录与覆盖模型的服务端分页。
    const [pricingDirectory, setPricingDirectory] = React.useState(null);
    // Committed pricing scope and page keep retained rows and pagination labels consistent.
    // 已提交定价作用域与页码确保保留记录和分页标签一致。
    const pricingDirectoryScope = React.useRef(null);
    const [pricingDirectoryPage, setPricingDirectoryPage] = React.useState(0);
    const [pricingDirectoryLoading, setPricingDirectoryLoading] = React.useState(false);
    const [pricingDirectoryError, setPricingDirectoryError] = React.useState("");
    // settingsError is independent of database loading so price configuration stays available.
    // settingsError 独立于数据库加载，让价格配置保持可用。
    const [settingsError, setSettingsError] = React.useState("");
    // Retry revisions rerun only the failed independent read boundary.
    // 重试修订号仅重新执行失败的独立读取边界。
    const [settingsRetry, setSettingsRetry] = React.useState(0);
    const [pricingRetry, setPricingRetry] = React.useState(0);
    // requestSequence rejects earlier responses after range changes or unmount.
    // requestSequence 拒绝范围变更或卸载后返回的旧响应。
    const requestSequence = React.useRef(0);
    // dashboardSubscription detaches superseded filters; the host owns shared execution.
    // dashboardSubscription 移除过期筛选订阅；宿主持有共享执行。
    const dashboardSubscription = React.useRef(null);
    // dashboard stores the latest schema-validated host statistics snapshot.
    // dashboard 保存最近一份通过 Schema 校验的宿主统计快照。
    const [dashboard, setDashboard] = React.useState(null);
    // dashboardKey identifies the exact filters of the retained successful snapshot.
    // dashboardKey 标识保留的成功快照所属的精确筛选条件。
    const dashboardKey = React.useRef(null);
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
    // settingsBusyRef prevents repeated writes before React commits disabled state.
    // settingsBusyRef 防止 React 提交禁用状态前重复写入。
    const settingsBusyRef = React.useRef(false);
    // consumerFilter selects one exact server-bound consumer identity.
    // consumerFilter 选择一个精确的服务端消费者身份。
    const [consumerFilter, setConsumerFilter] = React.useState(ALL_CONSUMERS);
    // loading prevents overlapping dashboard reads.
    // loading 防止仪表盘读取重叠。
    const [loading, setLoading] = React.useState(true);
    // Table navigation retains the dashboard and owns its own loading and error feedback.
    // 表格导航保留仪表盘，并独立管理加载与错误反馈。
    const [tableRefresh, setTableRefresh] = React.useState({ daily: false, recent: false, error: "" });
    // error stores the latest bounded host query diagnostic.
    // error 保存最近一条有界宿主查询诊断。
    const [error, setError] = React.useState("");
    // notice stores the latest successful price configuration result.
    // notice 保存最近一次价格配置成功结果。
    const [notice, setNotice] = React.useState("");
    // hiddenSeries stores user-hidden model lines without changing persisted statistics.
    // hiddenSeries 保存用户隐藏的模型折线，且不修改持久化统计。
    const [hiddenSeries, setHiddenSeries] = React.useState(() => new Set());
    // mountedRef prevents a completed host action from mutating an unmounted plugin node.
    // mountedRef 防止已完成宿主 Action 修改已经卸载的插件节点。
    const mountedRef = React.useRef(true);

    // refresh loads one coherent native dashboard revision through the plugin action boundary.
    // refresh 通过插件 Action 边界加载一个一致的原生仪表盘修订版。
    const refresh = React.useCallback(async (force = false) => {
      if (!mountedRef.current) return;
      // sequence captures the exact filter request represented by this response.
      // sequence 捕获当前响应代表的精确筛选请求。
      const sequence = ++requestSequence.current;
      dashboardSubscription.current?.abort();
      // subscription owns only this view's result delivery, not the underlying action.
      // subscription 仅持有此视图的结果投递，不持有底层 Action。
      const subscription = new AbortController();
      dashboardSubscription.current = subscription;
      // Track the exact tables changed relative to the last successfully displayed filters.
      // 相对最近成功展示的筛选条件，记录发生变化的精确表格。
      let changedTables = null;
      try {
        // argumentsValue captures the declared query contract for exact request deduplication.
        // argumentsValue 捕获声明的查询契约，以精确合并相同请求。
        // Each tab sends only its own filters, so a Provider choice never narrows recent attempts.
        // 每个标签页仅发送自身筛选，Provider 选择不会收窄最近请求。
        const argumentsValue = {
          view: dashboardView,
          startAtMs: timeRange.startAtMs, endAtMs: timeRange.endAtMs, consumerId: consumerFilter === ALL_CONSUMERS ? "" : consumerFilter,
          modelQuery, sortOrder,
          ...(dashboardView === "providers" ? { providerId: providerFilter, dailyOffset } : {}),
          ...(dashboardView === "recent" ? { recentOffset, recentModelType } : {}),
        };
        // key compares only immutable action arguments, never SQL or runtime guesses.
        // key 仅比较不可变 Action 参数，不使用 SQL 或运行时猜测。
        const key = JSON.stringify(argumentsValue);
        // Returning to the committed page invalidates pending navigation without reloading the dashboard.
        // 返回已提交分页时使待完成导航失效，不重新加载仪表盘。
        if (!force && dashboardKey.current === key) {
          setTableRefresh({ daily: false, recent: false, error: "" });
          return;
        }
        // Paging changes only the relevant table; range, search and view changes replace the scope.
        // 翻页仅改变对应表格；时间、搜索及页面切换会替换统计作用域。
        if (!force && dashboardKey.current !== null) {
          // Compare declared action arguments from the committed and requested pages.
          // 对比已提交分页与请求分页的声明式 Action 参数。
          const previous = JSON.parse(dashboardKey.current);
          const next = JSON.parse(key);
          if (["view", "startAtMs", "endAtMs", "consumerId", "providerId", "modelQuery", "sortOrder"].every((field) => previous[field] === next[field])) {
            // Both tables may change when the user navigates before an earlier response completes.
            // 用户在前一次响应完成前导航时，两张表格可能同时变化。
            const daily = previous.dailyOffset !== next.dailyOffset;
            const recent = previous.recentOffset !== next.recentOffset || previous.recentModelType !== next.recentModelType;
            if (daily || recent) changedTables = { daily, recent };
          }
        }
        setTableRefresh({ daily: changedTables?.daily ?? false, recent: changedTables?.recent ?? false, error: "" });
        setLoading(changedTables === null);
        if (changedTables === null) {
          setError("");
          if (dashboardKey.current !== key) { setDashboard(null); dashboardKey.current = null; }
        }
        // Request only the changed table projections; dashboard metadata remains owned by full refreshes.
        // 仅请求发生变化的表格投影；仪表盘元数据仍由完整刷新维护。
        if (changedTables !== null) {
          argumentsValue.view = changedTables.daily && changedTables.recent ? "detail-pages"
            : changedTables.daily ? "daily-page" : "recent-page";
        }
        // Host identity includes the actual partial projection and freezes null end once per execution.
        // 宿主身份包含实际局部投影，并在每次执行中只冻结一次空终点。
        const nextDashboard = await sdk.invoke("dashboard-get", argumentsValue, {
          refresh: force, signal: subscription.signal,
        });
        if (mountedRef.current && sequence === requestSequence.current) {
          dashboardKey.current = key;
          // Publish only the requested table slices; unrelated summaries and rows stay mounted.
          // 仅发布请求的表格切片；无关摘要和记录保持挂载。
          setDashboard((previous) => changedTables === null ? nextDashboard : {
            ...previous,
            ...(changedTables.daily ? { daily: nextDashboard.daily, dailyCount: nextDashboard.dailyCount } : {}),
            ...(changedTables.recent ? { recentAttempts: nextDashboard.recentAttempts, recentCount: nextDashboard.recentCount } : {}),
          });
          setTableRefresh({ daily: false, recent: false, error: "" });
        }
      } catch (reason) {
        if (mountedRef.current && sequence === requestSequence.current) {
          // Preserve the original host diagnostic within the failed request's UI boundary.
          // 在失败请求所属界面范围内保留宿主原始诊断。
          const message = reason instanceof Error ? reason.message : String(reason);
          if (changedTables === null) setError(message);
          else setTableRefresh({ ...changedTables, error: message });
        }
      } finally {
        if (mountedRef.current && sequence === requestSequence.current) setLoading(false);
      }
    }, [timeRange, consumerFilter, providerFilter, dailyOffset, recentOffset, recentModelType, modelQuery, sortOrder, dashboardView]);

    // Post-write refresh follows the currently rendered scope even if the user navigated during the write.
    // 即使用户在写入期间切换页面，写入后的刷新也遵循当前渲染作用域。
    const currentRefresh = React.useRef(refresh);
    currentRefresh.current = refresh;

    // selectPricingModel loads one exact override or catalog default into the string-only editor.
    // selectPricingModel 将一个精确覆盖或目录默认值加载到纯字符串编辑器。
    const selectPricingModel = React.useCallback((modelId) => {
      setSelectedPricingModel(modelId);
      setPricingDraft(editablePrices(pricingSettings, modelId));
      setNotice("");
      setSettingsError("");
    }, [pricingSettings]);

    // persistPricingOverrides saves one complete plugin-owned configuration and refreshes estimates.
    // persistPricingOverrides 保存完整插件自有配置并刷新估算结果。
    const persistPricingOverrides = React.useCallback(async (modelOverrides, successMessage) => {
      if (settingsBusyRef.current) return;
      settingsBusyRef.current = true;
      setSettingsBusy(true);
      setSettingsError("");
      setNotice("");
      try {
        const nextSettings = await sdk.invoke("settings-update", { modelOverrides });
        if (mountedRef.current) {
          setPricingSettings(nextSettings);
          setPricingDraft(editablePrices(nextSettings, selectedPricingModel));
          setNotice(successMessage);
          setSelectedPricingModel("");
        }
        await currentRefresh.current(true);
      } catch (reason) {
        if (mountedRef.current) setSettingsError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        settingsBusyRef.current = false;
        if (mountedRef.current) setSettingsBusy(false);
      }
    }, [selectedPricingModel]);

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

    // toggleSeries changes only local chart visibility and never reloads statistics.
    // toggleSeries 仅改变本地图表可见性，绝不重新加载统计。
    const toggleSeries = React.useCallback((id) => {
      setHiddenSeries((current) => {
        // next is an immutable visibility-set update.
        // next 是不可变的可见性集合更新。
        const next = new Set(current);
        if (next.has(id)) next.delete(id); else next.add(id);
        return next;
      });
    }, []);

    React.useEffect(() => {
      mountedRef.current = true;
      // active excludes a superseded retry even when the same component remains mounted.
      // active 排除过期重试，即使同一组件仍处于挂载状态。
      let active = true;
      setSettingsError("");
      // Price configuration resolves independently of statistics reads.
      // 价格配置独立于统计读取完成加载。
      void sdk.invoke("settings-get", {}).then((nextSettings) => {
        if (active) {
          setPricingSettings(nextSettings);
        }
      }).catch((reason) => {
        if (active) setSettingsError(reason instanceof Error ? reason.message : String(reason));
      });
      return () => {
        active = false;
        mountedRef.current = false;
      };
    }, [settingsRetry]);

    React.useEffect(() => {
      if (section !== "pricing") void refresh();
      return () => {
        requestSequence.current += 1;
        dashboardSubscription.current?.abort();
      };
    }, [refresh, section]);

    React.useEffect(() => {
      if (section !== "pricing" || !pricingSettings) return;
      // active and timer prevent stale search responses and coalesce rapid search edits.
      // active 与 timer 防止旧搜索响应，并合并快速搜索编辑。
      let active = true;
      // subscription cancels delivery and queued dispatch for obsolete pricing filters.
      // subscription 取消过期定价筛选的投递与尚未发送的请求。
      const subscription = new AbortController();
      setPricingDirectoryLoading(true);
      // A different search cannot inherit rows; another page of the same search retains them.
      // 不同搜索不能继承记录；相同搜索的另一分页保留已有记录。
      const scope = JSON.stringify({ search: pricingSearch, filter: pricingFilter });
      if (pricingDirectoryScope.current !== scope) {
        setPricingDirectory(null);
        setPricingDirectoryPage(0);
        pricingDirectoryScope.current = null;
      }
      setPricingDirectoryError("");
      const timer = window.setTimeout(() => {
        void sdk.invoke("pricing-get", { search: pricingSearch, filter: pricingFilter, offset: pricingPage * PRICING_PAGE_SIZE }, { signal: subscription.signal })
          .then((result) => { if (active) {
            pricingDirectoryScope.current = scope;
            setPricingDirectory(result);
            setPricingDirectoryPage(pricingPage);
          } })
          .catch((reason) => { if (active) setPricingDirectoryError(reason instanceof Error ? reason.message : String(reason)); })
          .finally(() => { if (active) setPricingDirectoryLoading(false); });
      }, 200);
      return () => { active = false; subscription.abort(); window.clearTimeout(timer); };
    }, [section, pricingSearch, pricingFilter, pricingPage, pricingSettings, pricingRetry]);

    // consumerIds contains exact server-bound identities observed in either dashboard projection.
    // consumerIds 包含任一仪表盘投影中观察到的精确服务端身份。
    const consumerIds = dashboard?.consumerIds ?? [];
    // selectedModelHasOverride controls the exact restore-default operation.
    // selectedModelHasOverride 控制精确的恢复默认操作。
    const selectedModelHasOverride = Boolean(
      selectedPricingModel && pricingSettings?.modelOverrides?.[selectedPricingModel],
    );
    // selectedPricingSource is derived from the complete settings authority, independent of the filtered directory page.
    // selectedPricingSource 派生自完整设置权威，与已筛选目录分页无关。
    const selectedPricingSource = !selectedPricingModel || !pricingSettings ? null : selectedModelHasOverride ? "override"
      : pricingSettings.catalogModels.some((entry) => entry.modelId === selectedPricingModel) ? "catalog" : "unpriced";
    // pricingDraftValid controls persistence without coercing decimal strings.
    // pricingDraftValid 控制持久化，且不强制转换十进制字符串。
    const pricingDraftValid = pricingDraftIsValid(pricingDraft);

    React.useEffect(() => {
      if (dashboard && consumerFilter !== ALL_CONSUMERS && !consumerIds.includes(consumerFilter)) {
        setConsumerFilter(ALL_CONSUMERS);
      }
    }, [consumerFilter, consumerIds, dashboard]);

    // Drop a Provider selection that the current scope no longer contains.
    // 当前作用域不再包含所选 Provider 时清除该选择。
    React.useEffect(() => {
      if (dashboard?.providers && providerFilter !== "" && !dashboard.providers.some((provider) => provider.providerId === providerFilter)) {
        setProviderFilter("");
        setDailyOffset(0);
      }
    }, [dashboard, providerFilter]);

    // providerTotal counts every attempt of the unfiltered Provider list for the "all" entry.
    // providerTotal 统计未筛选 Provider 列表的全部调用，用于「全部」项。
    const providerTotal = (dashboard?.providers ?? []).reduce((total, provider) => total + provider.attempts, 0);

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
    // Display counts from the committed page while a replacement page is pending or failed.
    // 新分页等待或失败时，计数继续采用已提交分页的位置。
    const committedPage = dashboardKey.current === null ? {} : JSON.parse(dashboardKey.current);
    const displayedPage = { dailyOffset: committedPage.dailyOffset ?? 0, recentOffset: committedPage.recentOffset ?? 0 };
    // summary is a display-only projection over already persisted host facts.
    // summary 是对已持久化宿主事实的纯展示投影。
    const summary = dashboard ? {
      ...dashboard.summary,
      successRate: dashboard.summary.attempts > 0 ? dashboard.summary.successes / dashboard.summary.attempts * 100 : null,
      usageCoverage: dashboard.summary.attempts > 0 ? dashboard.summary.reported / dashboard.summary.attempts * 100 : null,
    } : summarizeDaily([]);
    // heatmapCells contains the complete activity grid for the selected range.
    // heatmapCells 包含所选范围内完整对齐的活动点阵。
    const heatmapCells = buildHeatmapCells(dashboard?.heatmap ?? [], timeRange);
    // allSeries contains every runtime-projected model line.
    // allSeries 包含运行时投影的全部模型折线。
    const allSeries = buildLineSeries(dashboard?.series ?? [], timeRange);
    // visibleSeries excludes locally hidden lines without changing the action result.
    // visibleSeries 排除本地隐藏折线，但不改变 Action 结果。
    const visibleSeries = allSeries.filter((series) => !hiddenSeries.has(series.id));
    // legendItems projects every line into the shared visibility control model.
    // legendItems 将全部折线投影为共享可见性控件模型。
    const legendItems = allSeries.map((series) => ({ ...series, visible: !hiddenSeries.has(series.id) }));
    // visiblePricingModels contains only the declared server page, while totalCount covers the full filter.
    // visiblePricingModels 仅包含声明的服务端分页，totalCount 则覆盖完整筛选。
    const visiblePricingModels = pricingDirectory?.models ?? [];
    // pricingDirty compares exact strings so opening a default never creates an unsolicited override.
    // pricingDirty 比较精确字符串，确保打开默认值不会产生非预期覆盖。
    const pricingDirty = pricingSettings && PRICE_FIELDS.some((field) => pricingDraft[field.key] !== editablePrices(pricingSettings, selectedPricingModel)[field.key]);

    // refreshLabel names the toolbar refresh button for its current state.
    // refreshLabel 按当前状态命名工具栏刷新按钮。
    const refreshLabel = loading ? text("refreshing", "Refreshing") : text("refresh", "Refresh");
    // retryButton renders one recovery command for a notice.
    // retryButton 为提示渲染一个恢复命令。
    const retryButton = (onClick) => h(Button, { type: "button", size: "sm", variant: "outline", onClick }, text("error.retry", "Retry"));
    // tableStatus reports pending navigation of one paged table beside its section title.
    // tableStatus 在区块标题旁报告单个分页表格的待完成导航。
    const tableStatus = (kind) => tableRefresh[kind] && !tableRefresh.error
      ? h(PageHint, { role: "status" }, text("page.loading", "Updating this table…")) : null;
    // tableError keeps a failed navigation inside its owning section, above the retained rows.
    // tableError 将失败的导航保留在所属区块内，位于保留行之上。
    const tableError = (kind) => tableRefresh[kind] && tableRefresh.error
      ? h(PageNotice, { tone: "error", actions: retryButton(() => void refresh()) }, tableRefresh.error) : null;
    // searchForm renders the shared model search and time order used by both usage tabs.
    // searchForm 渲染两个用量标签页共用的模型搜索与时间排序。
    const searchForm = () => h("form", { role: "search", onSubmit: (event) => { event.preventDefault(); setModelQuery(modelDraft.trim()); setDailyOffset(0); setRecentOffset(0); } },
      h(PageToolbar, {
        actions: h(SelectControl, { ariaLabel: text("usage.order", "Time order"), value: sortOrder,
          options: ["newest", "oldest"].map((value) => ({ value, label: text(`usage.${value}`, value) })),
          onValueChange: (value) => { setSortOrder(value); setDailyOffset(0); setRecentOffset(0); } }),
      },
        h(Input, { density: "compact", type: "search", maxLength: 256,
          "aria-label": text("usage.search", "Search usage by model"), placeholder: text("usage.search", "Search usage by model"),
          value: modelDraft, onChange: (event) => setModelDraft(event.currentTarget.value) }),
        h(Button, { type: "submit", size: "sm", variant: "outline", disabled: loading }, text("usage.applySearch", "Search")),
      ),
    );
    // loadingPlaceholder stands in for dashboard content until the first snapshot arrives; a failed read shows only its notice.
    // loadingPlaceholder 在首个快照到达前代替仪表盘内容；读取失败时只显示其提示。
    const loadingPlaceholder = (withTiles) => error ? null : h(React.Fragment, null,
      withTiles ? h(StreamPlaceholder, { label: text("status.loading", "Loading system statistics"), shape: "tiles", count: 4 }) : null,
      h(StreamPlaceholder, withTiles ? { decorative: true, fill: true, shape: "rows" } : { label: text("status.loading", "Loading system statistics"), fill: true, shape: "rows" }));

    return h(PageBody, null,
      h(PageTabs, {
        ariaLabel: text("sections.label", "Statistics sections"),
        value: section,
        onValueChange: setSection,
        options: ["overview", "providers", "recent", "pricing"].map((value) => ({ value, label: text(`sections.${value}`, value) })),
        actions: h(StatusPill, {
          status: loading ? "loading" : error || dashboard?.healthStatus !== "ready" ? "warning" : "ready",
          label: loading
            ? text("status.loading", "Loading system statistics")
            : error
              ? text("status.error", "System statistics failed to load")
              : dashboard?.healthStatus === "ready" ? text("status.ready", "System statistics available") : text("status.unknown", "Statistics health is unknown"),
        }),
      }),
      section === "pricing" ? null : h(PageToolbar, {
        actions: h(React.Fragment, null,
          h(Button, {
            "aria-busy": loading, "aria-label": refreshLabel, disabled: loading, onClick: () => void refresh(true),
            size: "sm", title: refreshLabel, type: "button", variant: "outline",
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
          h(TimeRangeControl, {
            value: timeRange,
            onChange: (value) => { setTimeRange(value); setDailyOffset(0); setRecentOffset(0); },
          }),
          consumerIds.length > 0 ? h(SelectControl, {
            ariaLabel: text("consumer.label", "Consumer"),
            className: "plugin-statistics-toolbar-select",
            value: consumerFilter,
            options: [
              { value: ALL_CONSUMERS, label: text("consumer.all", "All consumers") },
              ...consumerIds.map((consumerId) => ({ value: consumerId, label: consumerLabel(consumerId) })),
            ],
            onValueChange: (value) => { setConsumerFilter(value); setDailyOffset(0); setRecentOffset(0); },
          }) : null,
        ),
      },
        dashboard ? h(Badge, { variant: "outline" }, text("source.host", "Host data")) : null,
        dashboard ? h(Badge, { variant: "outline" },
          `${text("pricing.catalog", "Reference catalog")} · ${currencyAmount("", dashboard.pricingCatalog.currency).trim()} · ${formatInteger(dashboard.pricingCatalog.entryCount)} + ${formatInteger(dashboard.pricingCatalog.overrideCount)}`) : null,
        modelQuery ? h(Badge, { variant: "outline" }, `${text("pricing.model", "Model")}: ${modelQuery}`) : null,
        modelQuery ? h(Button, { type: "button", size: "sm", variant: "ghost", onClick: () => {
          setModelQuery(""); setModelDraft(""); setDailyOffset(0); setRecentOffset(0);
        } }, text("usage.clearSearch", "Clear model search")) : null,
        dashboard ? h(PageHint, null, `${text("revision", "Revision")} ${formatInteger(dashboard.aggregateRevision)} · ${formatTimestamp(dashboard.generatedAtMs)}`) : null,
      ),
      section !== "pricing" && dashboard && (loading || error)
        ? h(PageHint, { role: "status" }, text("snapshot.retained", "Showing the previous result until a refresh succeeds.")) : null,
      error ? h(PageNotice, {
        tone: "error", title: text("error.title", "Unable to read system statistics"),
        details: error, detailsLabel: text("error.details", "Error details"),
        actions: loading ? null : retryButton(() => void refresh(true)),
      }) : null,
      settingsError ? h(PageNotice, {
        tone: "error", title: text("pricing.loadError", "Unable to load price settings"),
        details: settingsError, detailsLabel: text("error.details", "Error details"),
        actions: retryButton(() => setSettingsRetry((revision) => revision + 1)),
      }) : null,
      notice ? h(PageNotice, { tone: "success" }, notice) : null,
      h(PageTabPanel, { active: section === "overview" }, !dashboard ? loadingPlaceholder(true) : h(React.Fragment, null,
        h(MetricGrid, null,
          h(MetricCard, {
            icon: "requests",
            label: text("summary.requests", "Physical attempts"),
            value: formatInteger(summary.attempts),
            detail: text("summary.requestsDetail", "Includes retries and failures"),
          }),
          h(MetricCard, {
            icon: "success",
            label: text("summary.successRate", "Success rate"),
            value: summary.successRate === null ? "—" : formatPercent(summary.successRate),
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
            value: summary.usageCoverage === null ? "—" : formatPercent(summary.usageCoverage),
            detail: text("summary.usageCoverageDetail", "Directly Provider-reported"),
          }),
        ),
        h("div", { className: "plugin-statistics-balance" },
          h(PageSection, {
            title: text("cost.title", "Reference cost for the selected range"),
            actions: h(Button, { type: "button", variant: "ghost", size: "sm", onClick: () => setSection("pricing") }, text("sections.pricing", "Model pricing")),
          },
            ...dashboard.summary.costs.map((cost) => h("div", { className: "plugin-statistics-cost", key: cost.currency },
              h("strong", { className: "plugin-statistics-cost-value", title: cost.amount === null ? "" : cost.amount }, cost.amount === null ? "-" : currencyAmount(formatMoney(cost.amount, sdk.locale), cost.currency)),
              h(PageHint, null, interpolate(text("cost.coverage", "{{priced}} priced rows · {{unpriced}} unpriced · {{incomplete}} incomplete"), {
                priced: formatInteger(cost.pricedRows), unpriced: formatInteger(cost.unpricedRows), incomplete: formatInteger(cost.incompleteRows),
              })),
              cost.unpricedRows + cost.incompleteRows > 0 ? h(Badge, { variant: "outline" }, text("cost.partial", "Partial coverage")) : null,
            )),
            dashboard.summary.costs.length === 0 ? h(PageHint, null, text("cost.empty", "No cost records in this range")) : null,
          ),
          h(PageSection, { title: text("tokens.breakdown", "Input, output and cache") }, tokenOverview(summary)),
        ),
        h(PageSection, {
          title: text("heatmap.title", "Activity heatmap"),
          actions: h(ActivityHeatmapLegend, { lowLabel: text("heatmap.less", "Less"), highLabel: text("heatmap.more", "More") }),
        },
          h(ActivityHeatmap, {
            ariaLabel: text("heatmap.aria", "Token activity heatmap for the selected range"),
            cells: heatmapCells,
            emptyLabel: text("heatmap.empty", "No token activity yet"),
            formatMonth: formatMonthLabel,
            tooltipDelayMs: TOOLTIP_DELAY_MS,
          }),
        ),
        h(PageSection, { title: text("trend.title", "Daily token usage"), status: h(Badge, { variant: "outline" }, timeRangeLabel(timeRange)) },
          h(TimeSeriesLineChart, {
            ariaLabel: text("trend.aria", "Daily token usage line chart grouped by model"),
            emptyLabel: text("trend.empty", "No trend data in this range"),
            formatValue: formatInteger,
            series: visibleSeries,
            tooltipDelayMs: TOOLTIP_DELAY_MS,
          }),
          h(ChartLegend, { items: legendItems, onToggle: toggleSeries }),
        ),
      )),
      h(PageTabPanel, { active: section === "providers" },
        searchForm(),
        !dashboard ? loadingPlaceholder(false) : h("div", { className: "plugin-statistics-provider-layout" },
          h("nav", { className: "plugin-statistics-provider-list", "aria-label": text("providers.label", "Providers") },
            ...[{ providerId: "", attempts: providerTotal }, ...(dashboard.providers ?? [])].map((provider) => h(Button, {
              key: provider.providerId || ALL_CONSUMERS,
              type: "button", size: "sm",
              variant: providerFilter === provider.providerId ? "secondary" : "ghost",
              "aria-pressed": providerFilter === provider.providerId,
              className: "plugin-statistics-provider-option",
              disabled: loading,
              onClick: () => { setProviderFilter(provider.providerId); setDailyOffset(0); },
            },
              h("span", { title: provider.providerId }, provider.providerId || text("providers.all", "All providers")),
              h("small", null, formatInteger(provider.attempts)))),
          ),
          h("div", { className: "plugin-statistics-provider-main" },
            h(MetricGrid, null,
              h(MetricCard, {
                icon: "requests",
                label: text("summary.requests", "Physical attempts"),
                value: formatInteger(summary.attempts),
                detail: text("summary.requestsDetail", "Includes retries and failures"),
              }),
              h(MetricCard, {
                icon: "success",
                label: text("summary.successRate", "Success rate"),
                value: summary.successRate === null ? "—" : formatPercent(summary.successRate),
                detail: `${formatInteger(summary.successes)} ${text("summary.successes", "successful attempts")}`,
              }),
              h(MetricCard, {
                icon: "tokens",
                label: text("summary.tokens", "Known tokens"),
                value: formatInteger(summary.totalTokens),
                detail: text("summary.tokensDetail", "Unknown usage remains absent"),
              }),
              ...dashboard.summary.costs.map((cost) => h(MetricCard, {
                key: cost.currency,
                icon: "activity",
                label: text("cost.reference", "Reference cost"),
                value: cost.amount === null ? "-" : currencyAmount(formatMoney(cost.amount, sdk.locale), cost.currency),
                detail: cost.unpricedRows + cost.incompleteRows > 0 ? text("cost.partial", "Partial coverage") : undefined,
              })),
            ),
            h(PageSection, {
            title: text("daily.title", "Model usage"),
            "aria-busy": tableRefresh.daily && !tableRefresh.error,
            status: h(React.Fragment, null,
              h(Badge, { variant: "outline" }, `${displayedPage.dailyOffset + filteredDaily.length} / ${dashboard.dailyCount}`),
              tableStatus("daily")),
            footer: pageControls(displayedPage.dailyOffset, MAX_DAILY_ROWS, dashboard.dailyCount, setDailyOffset, loading || (tableRefresh.daily && !tableRefresh.error)),
          },
            tableError("daily"),
            h(PageTable, {
              label: text("daily.title", "Model usage"),
              layout: "fixed", minWidth: DETAIL_TABLE_MIN_WIDTH,
              columns: [
                { key: "model", label: text("daily.provider", "Provider / model"), width: "28%", wrap: true },
                { key: "consumer", label: text("daily.consumerOperation", "Consumer / operation"), width: "16%", wrap: true },
                { key: "attempts", label: text("daily.attempts", "Attempts"), width: "8%", numeric: true },
                { key: "tokens", label: text("daily.tokens", "Tokens"), width: "18%", wrap: true },
                { key: "duration", label: text("daily.averageDuration", "Average duration"), width: "20%", wrap: true },
                { key: "cost", label: text("daily.cost", "Reference estimate"), width: "10%", wrap: true },
              ],
              rows: filteredDaily.slice(0, MAX_DAILY_ROWS).map((row) => ({
                key: `${row.providerId}/${row.serviceEntryId}/${row.consumerId}/${row.operation}/${row.modelId}`,
                cells: {
                  model: modelIdentity(row),
                  consumer: h("div", { className: "plugin-statistics-consumer-operation" },
                    h("span", { title: row.consumerId }, consumerLabel(row.consumerId)),
                    h("span", { title: row.operation }, operationLabel(row.operation))),
                  attempts: `${formatInteger(row.successfulAttemptCount)} / ${formatInteger(row.attemptCount)}`,
                  tokens: tokenDetails(row, true),
                  duration: latencyDetails(row, true),
                  cost: estimateDetails(row),
                },
              })),
              empty: h(PageEmptyState, { title: text("daily.empty", "No usage in this range") }),
            }),
            ),
          ),
        ),
      ),
      h(PageTabPanel, { active: section === "recent" },
        searchForm(),
        !dashboard ? loadingPlaceholder(false) : h(PageSection, {
            title: text("recent.title", "Recent physical attempts"),
            "aria-busy": tableRefresh.recent && !tableRefresh.error,
            status: h(React.Fragment, null,
              h(Badge, { variant: "outline" }, `${displayedPage.recentOffset + filteredAttempts.length} / ${dashboard.recentCount}`),
              tableStatus("recent")),
            actions: h(SelectControl, {
              ariaLabel: text("recent.modelTypes", "Model types"),
              value: recentModelType,
              options: [{ value: "llm", label: "Chat models" }, { value: "embedding", label: "Embedding models" }, { value: "rerank", label: "Reranking models" }, { value: "decision", label: "Decision models" }]
                .map(({ value, label }) => ({ value, label: text(`recent.group.${value}`, label) })),
              onValueChange: (value) => { setRecentModelType(value); setRecentOffset(0); },
            }),
            footer: pageControls(displayedPage.recentOffset, MAX_RECENT_ATTEMPTS, dashboard.recentCount, setRecentOffset, loading || (tableRefresh.recent && !tableRefresh.error)),
          },
            tableError("recent"),
            h(PageTable, {
              label: text("recent.title", "Recent physical attempts"),
              layout: "fixed", minWidth: DETAIL_TABLE_MIN_WIDTH,
              columns: [
                { key: "time", label: text("recent.time", "Time"), width: "8%", wrap: true },
                { key: "model", label: text("recent.model", "Provider / model"), width: "24%", wrap: true },
                { key: "consumer", label: text("recent.consumer", "Consumer"), width: "10%", wrap: true },
                { key: "status", label: text("recent.status", "Status"), width: "7%", wrap: true },
                { key: "tokens", label: text("recent.tokens", "Tokens"), width: "17%", wrap: true },
                { key: "latency", label: text("timing.column", "Latency"), width: "21%", wrap: true },
                { key: "cost", label: text("recent.cost", "Reference estimate"), width: "9%", wrap: true },
                { key: "error", label: text("recent.error", "Error"), width: "4%" },
              ],
              rows: filteredAttempts.slice(0, MAX_RECENT_ATTEMPTS).map((attempt) => ({
                key: attempt.attemptId,
                cells: {
                  time: h("div", { className: "plugin-statistics-time" },
                    h("span", null, new Intl.DateTimeFormat(sdk.locale, { dateStyle: "short" }).format(new Date(attempt.terminalAtMs))),
                    h("small", null, new Intl.DateTimeFormat(sdk.locale, { timeStyle: "medium" }).format(new Date(attempt.terminalAtMs)))),
                  model: modelIdentity(attempt),
                  consumer: h("span", { title: attempt.consumerId }, consumerLabel(attempt.consumerId)),
                  status: h("span", { className: "plugin-statistics-status", "data-status": attempt.status }, text(`status.${attempt.status}`, attempt.status)),
                  tokens: tokenDetails(attempt),
                  latency: latencyDetails(attempt),
                  cost: estimateDetails(attempt),
                  error: errorDetails(attempt),
                },
              })),
              empty: h(PageEmptyState, { title: text("recent.empty", "No physical attempt records") }),
            }),
          ),
      ),
      h(PageTabPanel, { active: section === "pricing" },
        pricingSettings === null
          ? settingsError ? null : h(StreamPlaceholder, { label: text("pricing.loading", "Loading model prices"), shape: "rows", count: 8 })
          : h(PageSection, {
            title: text("pricing.title", "Model reference prices"),
            description: text("pricing.description", "Overrides belong to this plugin configuration. Estimates are recomputed and never written to the statistics database."),
            "aria-busy": pricingDirectoryLoading,
            status: pricingDirectoryLoading && pricingDirectory ? h(PageHint, { role: "status" }, text("pricing.loading", "Loading model prices")) : null,
            footer: pricingDirectory ? pageControls(pricingDirectoryPage * PRICING_PAGE_SIZE, PRICING_PAGE_SIZE, pricingDirectory.totalCount,
              (offset) => setPricingPage(offset / PRICING_PAGE_SIZE), pricingDirectoryLoading) : null,
          },
            h(PageToolbar, null,
              h(Input, { density: "compact", type: "search",
                "aria-label": text("pricing.search", "Search models"), placeholder: text("pricing.search", "Search models"),
                value: pricingSearch, onChange: (event) => { setPricingSearch(event.currentTarget.value); setPricingPage(0); } }),
              h(SelectControl, { ariaLabel: text("pricing.filter", "Price filter"), value: pricingFilter,
                options: ["all", "unpriced", "override", "used"].map((value) => ({ value, label: text(`pricing.filter.${value}`, value) })),
                onValueChange: (value) => { setPricingFilter(value); setPricingPage(0); } }),
              h(PageHint, null, text("pricing.tableScope", "Prices per 1M tokens. Used models include all retained usage."),
                pricingDirectory ? ` · ${currencyAmount("", pricingDirectory.currency).trim()}` : ""),
            ),
            pricingDirectoryError ? h(PageNotice, {
              tone: "error", title: text("pricing.loadError", "Unable to load model prices"),
              details: pricingDirectoryError, detailsLabel: text("error.details", "Error details"),
              actions: retryButton(() => setPricingRetry((revision) => revision + 1)),
            }) : null,
            pricingDirectoryLoading && !pricingDirectory
              ? h(StreamPlaceholder, { label: text("pricing.loading", "Loading model prices"), shape: "rows", count: 8 })
              : h(PageTable, {
                label: text("pricing.title", "Model reference prices"),
                columns: [
                  { key: "model", label: text("pricing.model", "Model"), wrap: true },
                  ...PRICE_FIELDS.map((field) => ({ key: field.key, label: text(field.messageKey, field.fallback), numeric: true })),
                  { key: "source", label: text("pricing.source", "Source") },
                  { key: "actions", label: text("pricing.actions", "Actions") },
                ],
                rows: visiblePricingModels.map((entry) => ({
                  key: entry.modelId,
                  selected: selectedPricingModel === entry.modelId,
                  cells: {
                    model: h(Button, { size: "sm", type: "button", variant: "ghost", disabled: settingsBusy, onClick: () => selectPricingModel(entry.modelId) }, entry.modelId),
                    ...Object.fromEntries(PRICE_FIELDS.map((field) => [field.key,
                      h("span", { title: entry.prices === null ? "" : entry.prices[field.key] }, formatMoney(entry.prices === null ? null : entry.prices[field.key], sdk.locale))])),
                    source: text(entry.source === "override" ? "pricing.override" : entry.source === "catalog" ? "pricing.default" : "pricing.unpriced", "Price source"),
                    actions: h(Button, { size: "sm", type: "button", variant: "outline", disabled: settingsBusy,
                      onClick: () => selectPricingModel(entry.modelId) }, text("pricing.edit", "Edit")),
                  },
                })),
                empty: pricingDirectory ? h(PageEmptyState, { title: text("pricing.empty", "No model identities available") }) : undefined,
              }),
          ),
        h(Dialog, { open: section === "pricing" && selectedPricingModel !== "", onOpenChange: (open) => {
          // Keep the selected model stable until the pending configuration write finishes.
          // 配置写入完成前保持所选模型稳定。
          if (!open && !settingsBusyRef.current) setSelectedPricingModel("");
        } }, h(DialogContent, { scrollMode: "body-scroll", className: "plugin-statistics-price-dialog" },
          h("section", { className: "plugin-statistics-price-editor", "aria-label": text("pricing.editor", "Model price editor") },
            h(DialogHeader, null,
              h("div", { className: "plugin-statistics-section-header" },
                h(DialogTitle, { className: "plugin-statistics-pricing-model" }, interpolate(text("pricing.editModel", "Edit model · {{model}}"), { model: selectedPricingModel })),
                h(Badge, { variant: "outline" }, selectedPricingSource === "override" ? text("pricing.override", "Custom override")
                  : selectedPricingSource === "catalog" ? text("pricing.default", "Catalog default") : text("pricing.unpriced", "Unpriced")),
              ),
              h(DialogDescription, null, text("pricing.description", "Enter all four prices, using zero only for explicitly free dimensions.")),
            ),
            h("div", { className: "plugin-statistics-price-grid" },
              ...PRICE_FIELDS.map((field) => h("label", { className: "plugin-statistics-price-field", key: field.key },
                h("span", null, `${text(field.messageKey, field.fallback)} · ${text("pricing.perMillion", "per 1M Token")}`),
                h(Input, { "aria-label": text(field.messageKey, field.fallback),
                  disabled: settingsBusy, inputMode: "decimal", maxLength: 64, spellCheck: false, type: "text", value: pricingDraft[field.key],
                  onChange: (event) => {
                    // Capture the exact string before React releases the event's currentTarget.
                    // 在 React 释放事件 currentTarget 前捕获精确字符串。
                    const value = event.currentTarget.value.trim();
                    setPricingDraft((current) => ({ ...current, [field.key]: value }));
                  },
                }),
              )),
            ),
            !pricingDraftValid ? h(PageHint, null, text("pricing.validation", "Enter four non-negative decimal prices.")) : null,
            settingsError ? h(PageNotice, { tone: "error" }, settingsError) : null,
            h(DialogFooter, null,
              h(Button, { disabled: settingsBusy || !selectedModelHasOverride, onClick: resetPricingOverride, size: "sm", type: "button", variant: "outline" }, text("pricing.reset", "Restore catalog default")),
              h(Button, { "aria-busy": settingsBusy, disabled: settingsBusy || !selectedPricingModel || !pricingDraftValid || !pricingDirty,
                onClick: savePricingOverride, size: "sm", type: "button" }, settingsBusy ? text("pricing.saving", "Saving") : text("pricing.save", "Save override")),
            ),
          ))),
      ),
    );
  }

  return UsageStatisticsPanel;
}
