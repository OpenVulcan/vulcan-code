import { PageEmptyState } from "@/components/page";

// BarChartPoint is one labelled column of a BarChart.
// BarChartPoint 是 BarChart 中一个带标签的柱。
export interface BarChartPoint {
  // key is unique within the chart.
  // key 在图表内唯一。
  key: string;
  // label is the short axis text under the column.
  // label 是柱下方的简短坐标轴文字。
  label: string;
  // value is the non-negative column height.
  // value 是非负柱高。
  value: number;
  // title is the hover and accessibility detail of the column.
  // title 是柱的悬停与无障碍详情。
  title?: string;
}

// BarChartProps describes one compact period column chart.
// BarChartProps 描述一个紧凑周期柱状图。
export interface BarChartProps {
  // ariaLabel names the chart image.
  // ariaLabel 命名图表图像。
  ariaLabel: string;
  // points are drawn left to right, oldest first.
  // points 从左到右绘制，最早的在前。
  points: readonly BarChartPoint[];
  // emptyLabel is shown when there are no points.
  // emptyLabel 在没有数据点时显示。
  emptyLabel: string;
}

// BAR_MIN_PERCENT keeps zero and tiny values visible as a sliver.
// BAR_MIN_PERCENT 让零值与极小值仍以细条可见。
const BAR_MIN_PERCENT = 3;

// BarChart renders theme-colored columns scaled to the largest value, with axis labels and native titles.
// BarChart 渲染按最大值缩放、使用主题颜色的柱形，并带坐标轴标签与原生标题提示。
export function BarChart(props: BarChartProps) {
  if (props.points.length === 0) return <PageEmptyState title={props.emptyLabel} />;
  const maximum = Math.max(1, ...props.points.map((point) => point.value));
  return <div aria-label={props.ariaLabel} className="plugin-bar-chart" role="img">
    {props.points.map((point) => <div className="plugin-bar-chart-column" key={point.key} title={point.title}>
      <span style={{ height: `${Math.max(BAR_MIN_PERCENT, (point.value / maximum) * 100)}%` }} />
      <small>{point.label}</small>
    </div>)}
  </div>;
}
