import { ANSWER_CHART_MAX_SERIES } from "@/generated/protocol-limits";
import type { AnswerChart } from "./answer-component-model";

// Drawing size of category charts in SVG user units; the SVG scales to the reply width.
// 分类图表的 SVG 绘图尺寸；SVG 随回答宽度缩放。
const WIDTH = 640;
const HEIGHT = 220;
const PAD = { top: 12, right: 10, bottom: 28, left: 46 };

// Series fill for index `index`; the palette lives in the answer component stylesheet.
// 第 `index` 个系列的填充色；色板定义在回答组件样式表中。
function seriesColor(index: number): string {
  return `var(--answer-series-${index + 1})`;
}

// Compact display text for one number.
// 单个数值的紧凑显示文本。
export function formatChartNumber(value: number): string {
  return Number(value.toPrecision(6)).toLocaleString();
}

// Evenly spaced axis ticks covering `low..high` with a 1/2/2.5/5 step.
// 以 1/2/2.5/5 步长均匀覆盖 `low..high` 的坐标刻度。
function ticks(low: number, high: number): number[] {
  const top = high === low ? low + 1 : high;
  const raw = (top - low) / 4;
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 2.5, 5, 10].map((factor) => factor * magnitude).find((candidate) => candidate >= raw) ?? raw;
  const values: number[] = [];
  for (let value = Math.floor(low / step) * step; value < top + step * 0.999; value += step) {
    values.push(Number(value.toPrecision(12)));
  }
  return values;
}

// Native hover text for one value.
// 单个数值的原生悬停提示。
function hint(category: string, series: string, value: number): string {
  return `${category} · ${series}: ${formatChartNumber(value)}`;
}

/** Draw a parsed chart: circular kinds as a ring or pie with a share legend, others on axes.
 * 绘制已解析图表：环形类为环图或饼图并附占比图例，其余在坐标轴上绘制。 */
export function AnswerChartView({ chart }: { chart: AnswerChart }) {
  if (chart.kind === "pie" || chart.kind === "donut") return <CircularChart chart={chart} />;
  return (
    <>
      {chart.series.length > 1 ? (
        <ul className="answer-chart-legend">
          {chart.series.map((name, index) => (
            <li key={name + index}><i style={{ background: seriesColor(index) }} />{name}</li>
          ))}
        </ul>
      ) : null}
      {chart.kind === "hbar" ? <HorizontalBars chart={chart} /> : <CategoryChart chart={chart} />}
    </>
  );
}

// Vertical bars, stacked bars, lines and areas over one category axis.
// 单一分类轴上的竖向条形、堆叠条形、折线与面积图。
function CategoryChart({ chart }: { chart: AnswerChart }) {
  const stacked = chart.kind === "stack";
  const values = chart.rows.flatMap((row) => (stacked ? [row.values.reduce((sum, value) => sum + value, 0)] : row.values));
  const axis = ticks(Math.min(0, ...values), Math.max(0, ...values));
  const [low, high] = [axis[0], axis[axis.length - 1]];
  const plotWidth = WIDTH - PAD.left - PAD.right;
  const plotHeight = HEIGHT - PAD.top - PAD.bottom;
  const y = (value: number) => PAD.top + ((high - value) / (high - low)) * plotHeight;
  const band = plotWidth / chart.rows.length;
  const center = (index: number) => PAD.left + band * (index + 0.5);
  const labelEvery = Math.ceil(chart.rows.length / 12);
  return (
    <svg className="answer-chart-svg" role="img" viewBox={`0 0 ${WIDTH} ${HEIGHT}`}>
      {axis.map((tick) => (
        <g key={tick}>
          <line className={tick === 0 ? "answer-chart-axis" : "answer-chart-grid"} x1={PAD.left} x2={WIDTH - PAD.right} y1={y(tick)} y2={y(tick)} />
          <text textAnchor="end" x={PAD.left - 8} y={y(tick) + 4}>{formatChartNumber(tick)}</text>
        </g>
      ))}
      {chart.kind === "bar" || stacked ? chart.rows.map((row, rowIndex) => {
        const group = band * 0.7;
        const width = stacked ? group : group / chart.series.length;
        let base = 0;
        return row.values.map((value, series) => {
          const from = stacked ? base : 0;
          const to = stacked ? (base += value) : value;
          const x = PAD.left + band * rowIndex + band * 0.15 + (stacked ? 0 : series * width);
          return (
            <rect fill={seriesColor(series)} height={Math.max(Math.abs(y(from) - y(to)), 0.5)} key={series} rx={2}
              width={Math.max(width - 2, 1)} x={x + 1} y={Math.min(y(from), y(to))}>
              <title>{hint(row.category, chart.series[series], value)}</title>
            </rect>
          );
        });
      }) : chart.series.map((name, series) => {
        const points = chart.rows.map((row, index) => `${center(index)},${y(row.values[series])}`);
        return (
          <g key={name + series}>
            {chart.kind === "area" ? (
              <path d={`M${points.join(" L")} L${center(chart.rows.length - 1)},${y(0)} L${center(0)},${y(0)}Z`} fill={seriesColor(series)} opacity={0.14} />
            ) : null}
            <path d={`M${points.join(" L")}`} fill="none" stroke={seriesColor(series)} strokeLinejoin="round" strokeWidth={2.4} />
            {chart.rows.map((row, index) => (
              <circle className="answer-chart-point" cx={center(index)} cy={y(row.values[series])} fill={seriesColor(series)} key={index} r={3.5}>
                <title>{hint(row.category, name, row.values[series])}</title>
              </circle>
            ))}
          </g>
        );
      })}
      {chart.rows.map((row, index) => (index % labelEvery === 0 ? (
        <text key={index} textAnchor="middle" x={center(index)} y={HEIGHT - 8}>{row.category}</text>
      ) : null))}
    </svg>
  );
}

// Horizontal grouped bars with category labels on the left.
// 左侧带分类标签的横向分组条形图。
function HorizontalBars({ chart }: { chart: AnswerChart }) {
  const values = chart.rows.flatMap((row) => row.values);
  const axis = ticks(Math.min(0, ...values), Math.max(0, ...values));
  const [low, high] = [axis[0], axis[axis.length - 1]];
  const labelWidth = 128;
  const barHeight = 14;
  const rowHeight = barHeight * chart.series.length + 12;
  const height = PAD.top + rowHeight * chart.rows.length + 22;
  const plotWidth = WIDTH - labelWidth - PAD.right;
  const x = (value: number) => labelWidth + ((value - low) / (high - low)) * plotWidth;
  const bottom = height - 22;
  return (
    <svg className="answer-chart-svg" role="img" viewBox={`0 0 ${WIDTH} ${height}`}>
      {axis.map((tick, index) => (
        <g key={tick}>
          <line className={tick === 0 ? "answer-chart-axis" : "answer-chart-grid"} x1={x(tick)} x2={x(tick)} y1={PAD.top} y2={bottom} />
          {/* The last tick sits on the right edge, so it ends there instead of centering.
              最后一个刻度位于右边缘，因此右对齐而非居中。 */}
          <text textAnchor={index === axis.length - 1 ? "end" : "middle"} x={x(tick)} y={height - 6}>{formatChartNumber(tick)}</text>
        </g>
      ))}
      {chart.rows.map((row, rowIndex) => {
        const top = PAD.top + rowHeight * rowIndex + 6;
        const label = row.category.length > 18 ? `${row.category.slice(0, 17)}…` : row.category;
        return (
          <g key={rowIndex}>
            <text textAnchor="end" x={labelWidth - 8} y={top + (barHeight * chart.series.length) / 2 + 4}>
              {label}<title>{row.category}</title>
            </text>
            {row.values.map((value, series) => (
              <rect fill={seriesColor(series)} height={barHeight - 2} key={series} rx={2}
                width={Math.max(Math.abs(x(value) - x(0)), 0.5)} x={Math.min(x(0), x(value))} y={top + series * barHeight}>
                <title>{hint(row.category, chart.series[series], value)}</title>
              </rect>
            ))}
          </g>
        );
      })}
    </svg>
  );
}

// Pie or ring with one share per category; the ring shows the total in its center.
// 每个分类一个占比的饼图或环图；环图在中心显示合计。
function CircularChart({ chart }: { chart: AnswerChart }) {
  const total = chart.rows.reduce((sum, row) => sum + row.values[0], 0);
  const donut = chart.kind === "donut";
  // A pie is a ring whose stroke covers the whole radius.
  // 饼图是描边覆盖整个半径的环。
  const radius = donut ? 44 : 30;
  const stroke = donut ? 24 : 60;
  const circumference = 2 * Math.PI * radius;
  let offset = 0;
  return (
    <div className="answer-chart-circular">
      <svg role="img" viewBox="0 0 120 120">
        {chart.rows.map((row, index) => {
          const length = (row.values[0] / total) * circumference;
          const segment = (
            <circle cx={60} cy={60} fill="none" key={index} r={radius} stroke={seriesColor(index % ANSWER_CHART_MAX_SERIES)}
              strokeDasharray={`${length} ${circumference - length}`} strokeDashoffset={-offset} strokeWidth={stroke} transform="rotate(-90 60 60)">
              <title>{hint(row.category, chart.series[0], row.values[0])}</title>
            </circle>
          );
          offset += length;
          return segment;
        })}
        {donut ? <text className="answer-chart-total" textAnchor="middle" x={60} y={65}>{formatChartNumber(total)}</text> : null}
      </svg>
      <ul className="answer-chart-legend" data-layout="column">
        {chart.rows.map((row, index) => (
          <li key={index}>
            <i style={{ background: seriesColor(index % ANSWER_CHART_MAX_SERIES) }} />{row.category}
            <b>{`${Math.round((row.values[0] / total) * 100)}%`}</b>
          </li>
        ))}
      </ul>
    </div>
  );
}
