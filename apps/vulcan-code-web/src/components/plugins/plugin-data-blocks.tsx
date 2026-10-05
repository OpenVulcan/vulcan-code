import * as React from "react";
import { Button } from "@/components/ui/button";
import { MetricCard, MetricGrid, PageEmptyState, PageTable } from "@/components/page";
import { useI18n } from "@/i18n";
import { BarChart } from "./plugin-bar-chart";
import { HelpTooltip, TimeSeriesLineChart } from "./plugin-host-components";
import type { PluginMessage } from "./plugin-settings-host";
import {
  DEFAULT_CHART_LIMIT, formatDataValue, interpolateCount, metricValue, readNumber, readPath, readRows,
  type PluginDataBlock,
} from "./plugin-data-view";

// PluginDataBlockView renders one validated data block for the WebUI, honoring only its `web` hints.
// PluginDataBlockView 为 WebUI 渲染一个已校验数据块，只采用其 `web` 提示。
export function PluginDataBlockView(props: { block: PluginDataBlock; result: unknown; message: PluginMessage }) {
  const { block } = props;
  if (block.web?.hidden) return null;
  switch (block.type) {
    case "metrics": return <MetricsBlock block={block} message={props.message} result={props.result} />;
    case "chart": return <ChartBlock block={block} message={props.message} result={props.result} />;
    case "table": return <TableBlock block={block} message={props.message} result={props.result} />;
  }
}

// BlockHeading titles a block, with optional secondary help or a trailing note.
// BlockHeading 为数据块加标题，可带次要帮助或尾部说明。
function BlockHeading(props: { title: string; help?: string; note?: string }) {
  return <div className="plugin-data-heading">
    <h4>{props.title}{props.help ? <HelpTooltip ariaLabel={props.title} content={props.help} /> : null}</h4>
    {props.note ? <span>{props.note}</span> : null}
  </div>;
}

// MetricsBlock shows key figures as host metric cards or, with the `tiles` variant, compact tinted tiles.
// MetricsBlock 以宿主指标卡片显示关键数字；`tiles` 变体显示紧凑着色方块。
function MetricsBlock(props: { block: Extract<PluginDataBlock, { type: "metrics" }>; result: unknown; message: PluginMessage }) {
  const { locale } = useI18n();
  const { block, message } = props;
  const source = block.source_path ? readPath(props.result, block.source_path) : props.result;
  const title = block.title_message_key ? message(block.title_message_key) : null;
  const missing = block.source_path !== undefined && (source === null || typeof source !== "object");
  const columns = block.web?.columns ? { "--plugin-data-columns": block.web.columns } as React.CSSProperties : undefined;
  let body: React.ReactNode;
  if (missing) {
    body = <PageEmptyState title={message(block.empty_message_key ?? "")} />;
  } else if (block.web?.variant === "tiles") {
    body = <div className="plugin-data-tiles" data-columns={block.web.columns ? "" : undefined} style={columns}>
      {block.items.map((item) => <div className="plugin-data-tile" key={item.label_message_key}>
        <span>{message(item.label_message_key)}</span>
        <strong>{metricValue(source, item, locale)}</strong>
      </div>)}
    </div>;
  } else {
    body = <MetricGrid className="plugin-data-cards" data-columns={block.web?.columns ? "" : undefined} style={columns}>
      {block.items.map((item) => <MetricCard key={item.label_message_key} label={message(item.label_message_key)} value={metricValue(source, item, locale)} />)}
    </MetricGrid>;
  }
  return title === null ? body : <section className="plugin-data-block">
    <BlockHeading help={block.help_message_key ? message(block.help_message_key) : undefined} title={title} />
    {body}
  </section>;
}

// ChartBlock switches between declared periods locally and draws the latest points as bars or a line.
// ChartBlock 在本地切换已声明周期，并以柱形或折线绘制最近的数据点。
function ChartBlock(props: { block: Extract<PluginDataBlock, { type: "chart" }>; result: unknown; message: PluginMessage }) {
  const { locale, t } = useI18n();
  const { block, message } = props;
  const [periodId, setPeriodId] = React.useState(block.periods[0]?.id ?? "");
  const period = block.periods.find((candidate) => candidate.id === periodId) ?? block.periods[0];
  const valueLabel = message(block.value_label_message_key);
  const valueFormat = block.value_format ?? "number";
  const points = readRows(props.result, period.path).slice(-(block.limit ?? DEFAULT_CHART_LIMIT)).map((row, index) => {
    const label = period.label_paths.map((path) => formatDataValue(readPath(row, path), period.label_format ?? "text", locale)).join(" – ");
    const value = readNumber(row, block.value_path) ?? 0;
    const detail = block.detail ? ` · ${formatDataValue(readPath(row, block.detail.path), block.detail.format, locale)}` : "";
    return { key: `${label}:${index}`, label, value, title: `${label}: ${formatDataValue(value, valueFormat, locale)}${detail}` };
  });
  const empty = message(block.empty_message_key);
  return <section className="plugin-data-block">
    {block.title_message_key ? <BlockHeading title={message(block.title_message_key)} /> : null}
    {block.periods.length > 1 ? <div aria-label={t("plugins.data.periods")} className="plugin-data-periods" role="group">
      {block.periods.map((candidate) => <Button aria-pressed={candidate.id === period.id} key={candidate.id} onClick={() => setPeriodId(candidate.id)}
        size="sm" type="button" variant={candidate.id === period.id ? "secondary" : "ghost"}>{message(candidate.label_message_key)}</Button>)}
    </div> : null}
    {block.web?.variant === "line"
      ? <TimeSeriesLineChart ariaLabel={valueLabel} emptyLabel={empty} formatValue={(value: number) => formatDataValue(value, valueFormat, locale)}
        series={[{ id: block.value_path, label: valueLabel, color: "var(--primary)", points: points.map((point) => ({ key: point.label, value: point.value })) }]} />
      : <BarChart ariaLabel={valueLabel} emptyLabel={empty} points={points} />}
  </section>;
}

// TableBlock lists one result array with an optional rank column; `bar` cells compare against their column maximum.
// TableBlock 列出一个结果数组，可带名次列；`bar` 单元格与本列最大值比较。
function TableBlock(props: { block: Extract<PluginDataBlock, { type: "table" }>; result: unknown; message: PluginMessage }) {
  const { locale } = useI18n();
  const { block, message } = props;
  const rows = readRows(props.result, block.path);
  const maxima = block.columns.map((column) => column.format === "bar" ? Math.max(1, ...rows.map((row) => readNumber(row, column.path) ?? 0)) : 1);
  const numeric = (format: string | undefined) => !["text", "code", "date", "month", "bar", undefined].includes(format);
  const title = block.title_message_key ? message(block.title_message_key) : null;
  const columns = [
    ...(block.rank ? [{ key: "rank", label: "#", numeric: true }] : []),
    ...block.columns.map((column, index) => ({ key: `c${index}`, label: message(column.label_message_key), numeric: numeric(column.format) })),
  ];
  const table = <PageTable columns={columns} empty={<PageEmptyState title={message(block.empty_message_key)} />} label={title ?? undefined}
    rows={rows.map((row, rowIndex) => ({
      key: String(rowIndex),
      cells: Object.fromEntries([
        ...(block.rank ? [["rank", new Intl.NumberFormat(locale).format(rowIndex + 1)]] : []),
        ...block.columns.map((column, index) => {
          const value = readPath(row, column.path);
          if (column.format === "bar") {
            const width = Math.max(4, ((readNumber(row, column.path) ?? 0) / maxima[index]) * 100);
            return [`c${index}`, <span aria-hidden className="plugin-data-bar" key="bar"><span style={{ width: `${width}%` }} /></span>];
          }
          const text = formatDataValue(value, column.format ?? "text", locale);
          return [`c${index}`, column.format === "code" ? <code>{text}</code> : text];
        }),
      ]),
    }))} />;
  return title === null ? table : <section className="plugin-data-block">
    <BlockHeading note={block.count_message_key && rows.length > 0 ? interpolateCount(message(block.count_message_key), new Intl.NumberFormat(locale).format(rows.length)) : undefined}
      title={title} />
    {table}
  </section>;
}
