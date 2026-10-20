import * as React from "react";
import { PageEmptyState } from "@/components/page";
import {
  CircleHelp,
  CircleAlert,
  ExternalLink,
  FileText,
  Github,
} from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/ui-kit/primitives/select";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/ui-kit/primitives/tooltip";

// HelpTooltipProps describes one host-rendered contextual-help icon for plugin interfaces.
// HelpTooltipProps 描述插件界面使用的宿主渲染上下文帮助图标。
export interface HelpTooltipProps {
  // Accessible label announced for the focusable question-mark trigger.
  // 可聚焦问号触发器对辅助技术声明的标签。
  ariaLabel: string;
  // Localized text or structured facts displayed inside the tooltip.
  // Tooltip 内显示的本地化文字或结构化信息。
  content: React.ReactNode;
  // Error details use a warning glyph; ordinary help retains the question mark.
  // 错误详情使用警示图标，普通帮助保留问号。
  tone?: "default" | "error";
  // Hover delay in milliseconds before the tooltip opens.
  // Tooltip 打开前的鼠标悬停延迟毫秒数。
  delayMs?: number;
}

// HelpTooltip renders one reusable circled-question trigger through the host tooltip layer.
// HelpTooltip 通过宿主 Tooltip 层渲染一个可复用的圆圈问号触发器。
//
// Parameters: props supplies accessible text, localized content, and an optional open delay.
// 参数：props 提供无障碍文字、本地化内容和可选的打开延迟。
// Returns: one keyboard-focusable help icon with a portal-backed tooltip.
// 返回值：返回一个带 Portal Tooltip 且可通过键盘聚焦的帮助图标。
export function HelpTooltip(props: HelpTooltipProps) {
  return (
    <Tooltip delayDuration={props.delayMs ?? 500}>
      <TooltipTrigger asChild>
        <button aria-label={props.ariaLabel} className="plugin-help-tooltip-trigger" data-tone={props.tone} type="button">
          {props.tone === "error" ? <CircleAlert aria-hidden="true" size={14} /> : <CircleHelp aria-hidden="true" size={14} />}
        </button>
      </TooltipTrigger>
      <TooltipContent className="plugin-help-tooltip-content" side="top" sideOffset={6}>
        {props.content}
      </TooltipContent>
    </Tooltip>
  );
}

// PluginMetadataLinkIcon identifies the bounded host icon set for plugin metadata tags.
// PluginMetadataLinkIcon 标识插件元数据标签可使用的有界宿主图标集合。
export type PluginMetadataLinkIcon = "github" | "license" | "link" | "document";

// PluginMetadataLinkProps describes one compact actionable metadata tag.
// PluginMetadataLinkProps 描述一个紧凑的可操作元数据标签。
export interface PluginMetadataLinkProps {
  // Localized visible tag label.
  // 本地化可见标签文案。
  label: string;
  // Accessible action label.
  // 可访问操作标签。
  ariaLabel: string;
  // Stable host icon selected for this metadata kind.
  // 为当前元数据类型选择的稳定宿主图标。
  icon: PluginMetadataLinkIcon;
  // Optional external destination retained for native link semantics.
  // 为原生链接语义保留的可选外部目标地址。
  href?: string;
  // Runs the owning plugin's host-authorized action.
  // 执行所属插件经宿主授权的操作。
  onActivate: () => void;
  // Whether the metadata action currently rejects interaction.
  // 元数据操作当前是否拒绝交互。
  disabled?: boolean;
}

// PluginMetadataLink renders one host-themed compact link or button tag.
// PluginMetadataLink 渲染一个采用宿主主题的紧凑链接或按钮标签。
//
// Parameters: props supplies the icon, label, optional URL, and authorized activation callback.
// 参数：props 提供图标、标签、可选地址与经授权的激活回调。
// Returns: one keyboard-accessible metadata tag with stable host styling.
// 返回值：返回一个具备稳定宿主样式且可通过键盘访问的元数据标签。
export function PluginMetadataLink(props: PluginMetadataLinkProps) {
  // Icon resolves the bounded metadata kind to one current host icon component.
  // Icon 将有界元数据类型解析为当前宿主图标组件。
  const Icon = props.icon === "github" ? Github : props.icon === "link" ? ExternalLink : FileText;
  // content is shared by native links and local dialog buttons.
  // content 由原生链接与本地浮动框按钮共享。
  const content = (
    <>
      <Icon aria-hidden="true" size={13} />
      <span>{props.label}</span>
    </>
  );
  if (props.href) {
    return (
      <a
        aria-label={props.ariaLabel}
        className="plugin-metadata-link"
        href={props.href}
        onClick={(event) => {
          event.preventDefault();
          props.onActivate();
        }}
        rel="noreferrer"
        target="_blank"
      >
        {content}
      </a>
    );
  }
  return (
    <button
      aria-label={props.ariaLabel}
      className="plugin-metadata-link"
      disabled={props.disabled}
      onClick={props.onActivate}
      type="button"
    >
      {content}
    </button>
  );
}

// TextDocumentDialogProps describes one host-rendered read-only text document dialog.
// TextDocumentDialogProps 描述一个由宿主渲染的只读文本文档浮动框。
export interface TextDocumentDialogProps {
  // Whether the dialog is visible.
  // 浮动框当前是否可见。
  open: boolean;
  // Receives the next controlled visibility state.
  // 接收下一个受控可见状态。
  onOpenChange: (open: boolean) => void;
  // Localized document title.
  // 本地化文档标题。
  title: string;
  // Optional localized supporting description.
  // 可选本地化辅助说明。
  description?: string;
  // Untouched document content displayed in a selectable monospace area.
  // 显示在可选择等宽区域内的原样文档内容。
  content: string;
  // Whether the host is waiting for the document resource.
  // 宿主当前是否正在等待文档资源。
  loading?: boolean;
  // Localized loading label.
  // 本地化加载文案。
  loadingLabel?: string;
  // Optional bounded resource-load error.
  // 可选的有界资源加载错误。
  error?: string;
}

// TextDocumentDialog renders one selectable read-only document through the host dialog layer.
// TextDocumentDialog 通过宿主浮动框层渲染一个可选择的只读文档。
//
// Parameters: props controls visibility and supplies localized metadata plus current resource state.
// 参数：props 控制可见性，并提供本地化元数据与当前资源状态。
// Returns: one portal-backed modal whose document body preserves package text formatting.
// 返回值：返回一个通过 Portal 渲染且保留包文本格式的模态浮动框。
export function TextDocumentDialog(props: TextDocumentDialogProps) {
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="plugin-text-document-dialog" size="lg" scrollMode="content-scroll">
        <DialogHeader>
          <DialogTitle>{props.title}</DialogTitle>
          {props.description ? <DialogDescription>{props.description}</DialogDescription> : null}
        </DialogHeader>
        {props.loading ? (
          <div className="plugin-text-document-state" role="status">{props.loadingLabel ?? "Loading"}</div>
        ) : props.error ? (
          <div className="plugin-text-document-state error" role="alert">{props.error}</div>
        ) : (
          <pre className="plugin-text-document-content">{props.content}</pre>
        )}
      </DialogContent>
    </Dialog>
  );
}

// SelectControlOption describes one stable value-label choice in a plugin select.
// SelectControlOption 描述插件下拉控件中的一个稳定值标签选项。
export interface SelectControlOption {
  // Stable machine value returned to the plugin.
  // 返回给插件的稳定机器值。
  value: string;
  // Localized user-visible option label.
  // 本地化用户可见选项标签。
  label: string;
  // Whether the option is unavailable for selection.
  // 当前选项是否不可选择。
  disabled?: boolean;
}

// SelectControlProps describes one host-themed plugin select control.
// SelectControlProps 描述一个采用宿主主题的插件下拉控件。
export interface SelectControlProps {
  // Current selected machine value.
  // 当前选中的机器值。
  value: string;
  // Ordered choices rendered by the host component.
  // 由宿主组件渲染的有序选项。
  options: SelectControlOption[];
  // Localized accessible control name.
  // 本地化可访问控件名称。
  ariaLabel: string;
  // Receives the exact selected machine value.
  // 接收精确选中的机器值。
  onValueChange: (value: string) => void;
  // Optional trigger class for plugin-owned layout sizing.
  // 用于插件自有布局尺寸的可选触发器类名。
  className?: string;
  // Host-defined compact or default control height.
  // 宿主定义的紧凑或默认控件高度。
  size?: "sm" | "default";
  // Whether the control rejects user interaction during an active operation.
  // 控件是否在活动操作期间拒绝用户交互。
  disabled?: boolean;
}

// SelectControl renders a stable design-system select without exposing Radix primitives to plugins.
// SelectControl 使用稳定设计系统渲染下拉控件，不向插件暴露 Radix 原语。
//
// Parameters: props contains the selected value, localized options, accessible label, and change callback.
// 参数：props 包含选中值、本地化选项、可访问标签与变更回调。
// Returns: a controlled host select whose popup inherits the active Vulcan theme.
// 返回值：返回弹层继承当前 Vulcan 主题的受控宿主下拉控件。
export function SelectControl(props: SelectControlProps) {
  return (
    <Select disabled={props.disabled} value={props.value} onValueChange={props.onValueChange}>
      <SelectTrigger aria-label={props.ariaLabel} className={props.className} size={props.size ?? "sm"}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {props.options.map((option) => (
          <SelectItem disabled={option.disabled} key={option.value} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

// MetricCard is the page-kit KPI card; the plugin SDK exposes the same component.
// MetricCard 是页面套件 KPI 卡片；插件 SDK 暴露同一组件。
export { MetricCard, type MetricCardIconName, type MetricCardProps } from "@/components/page";

// ActivityHeatmapCell describes one calendar-day activity point.
// ActivityHeatmapCell 描述一个自然日活动点。
export interface ActivityHeatmapCell {
  // ISO calendar date used for tooltips and accessibility.
  // 用于提示与可访问性的 ISO 自然日。
  date: string;
  // Stable intensity from zero through five.
  // 从零到五的稳定强度。
  intensity: number;
  // Localized accessible tooltip text.
  // 本地化可访问提示文本。
  label: string;
}

// ActivityHeatmapProps describes one weekly-column calendar heatmap.
// ActivityHeatmapProps 描述一个按周分列的日历热力图。
export interface ActivityHeatmapProps {
  // Ordered calendar cells, normally the latest 365 days.
  // 有序日历单元，通常为最近 365 天。
  cells: ActivityHeatmapCell[];
  // Accessible chart label.
  // 可访问图表标签。
  ariaLabel: string;
  // Optional empty-state label.
  // 可选空状态标签。
  emptyLabel?: string;
  // Optional localized month-label formatter receiving an ISO calendar date.
  // 可选的本地化月份标签格式器，接收 ISO 自然日。
  formatMonth?: (date: string) => string;
  // Optional hover delay before a cell tooltip opens.
  // 单元格 Tooltip 打开前的可选悬停延迟。
  tooltipDelayMs?: number;
}

// ActivityHeatmapLegendProps describes localized activity intensity endpoints.
// ActivityHeatmapLegendProps 描述本地化活动强度两端标签。
export interface ActivityHeatmapLegendProps {
  // Label for the lowest activity intensity.
  // 最低活动强度标签。
  lowLabel: string;
  // Label for the highest activity intensity.
  // 最高活动强度标签。
  highLabel: string;
}

// ActivityHeatmapLegend renders the same five-level scale used by the shared heatmap.
// ActivityHeatmapLegend 渲染共享热力图使用的同一套五级色阶。
//
// Parameters: props provides localized labels for both ends of the intensity scale.
// 参数：props 提供强度色阶两端的本地化标签。
// Returns: a compact non-interactive legend aligned with plugin section headers.
// 返回值：返回与插件区块标题对齐的紧凑非交互图例。
export function ActivityHeatmapLegend(props: ActivityHeatmapLegendProps) {
  return (
    <div className="plugin-activity-legend" aria-label={`${props.lowLabel} – ${props.highLabel}`}>
      <span>{props.lowLabel}</span>
      {[1, 2, 3, 4, 5].map((intensity) => (
        <span aria-hidden="true" className="plugin-activity-cell" data-intensity={intensity} key={intensity} />
      ))}
      <span>{props.highLabel}</span>
    </div>
  );
}

// ActivityHeatmap renders short ranges by day and longer ranges as a keyboard-focusable weekly calendar.
// ActivityHeatmap 按日显示短范围，并将较长范围显示为支持键盘聚焦的按周日历。
// Parameters: props contains the ordered real range and localized labels. Returns: every supplied day exactly once.
// 参数：props 包含有序真实范围与本地化名称。返回值：每个提供的日期恰好出现一次。
export function ActivityHeatmap(props: ActivityHeatmapProps) {
  if (props.cells.length === 0) {
    return <PageEmptyState title={props.emptyLabel ?? "No activity"} />;
  }
  // firstDate is the exact local calendar date represented by the first cell.
  // firstDate 是首个单元表示的精确本地自然日。
  const firstDate = new Date(`${props.cells[0].date}T00:00:00`);
  // leadingEmptyCellCount aligns the first real date with its weekday without inventing data days.
  // leadingEmptyCellCount 将首个真实日期对齐到星期行，且不虚构数据日期。
  const leadingEmptyCellCount = firstDate.getDay();
  // weekColumnCount is the complete number of rendered weekly columns.
  // weekColumnCount 是渲染出的完整周列数。
  const weekColumnCount = Math.ceil((leadingEmptyCellCount + props.cells.length) / 7);
  // dailyLayout gives a month-or-shorter range meaningful horizontal space without reserving annual columns.
  // dailyLayout 为不超过一个月的范围提供有意义的横向空间，不预留年度列。
  const dailyLayout = props.cells.length <= 31;
  // columns matches labels and data positions to the actual rendered range.
  // columns 将标签和数据位置与实际渲染范围对齐。
  const columns = dailyLayout ? `repeat(${props.cells.length}, minmax(18px, 1fr))` : `repeat(${weekColumnCount}, 11px)`;
  // monthLabels contains the first visible month and every subsequent month boundary.
  // monthLabels 包含首个可见月份与此后的每个月份边界。
  const monthLabels = props.cells.flatMap((cell, index) => {
    if (index !== 0 && !cell.date.endsWith("-01")) {
      return [];
    }
    return [{
      column: dailyLayout ? index + 1 : Math.floor((leadingEmptyCellCount + index) / 7) + 1,
      date: cell.date,
      label: props.formatMonth ? props.formatMonth(cell.date) : cell.date.slice(0, 7),
    }];
  });
  return (
    <div className="plugin-activity-heatmap-scroll">
      <div className="plugin-activity-heatmap-content" data-layout={dailyLayout ? "days" : "weeks"}
        style={dailyLayout ? { minWidth: props.cells.length * 18 + (props.cells.length - 1) * 3 } : undefined}>
        <div
          aria-hidden="true"
          className="plugin-activity-month-labels"
          style={{ gridTemplateColumns: columns }}
        >
          {monthLabels.map((month) => (
            <span key={month.date} style={{ gridColumnStart: month.column }}>{month.label}</span>
          ))}
        </div>
        <div className="plugin-activity-heatmap" role="grid" aria-label={props.ariaLabel}
          style={dailyLayout ? { gridTemplateColumns: columns } : undefined}>
          {Array.from({ length: dailyLayout ? 0 : leadingEmptyCellCount }, (_, index) => (
            <span aria-hidden="true" className="plugin-activity-cell placeholder" key={`placeholder-${index}`} />
          ))}
          {props.cells.map((cell) => {
            // intensity is clamped to the host-defined zero-through-five color scale.
            // intensity 被限制到宿主定义的零至五颜色等级。
            const intensity = Math.max(0, Math.min(5, Math.trunc(cell.intensity)));
            return (
              <Tooltip delayDuration={props.tooltipDelayMs ?? 0} key={cell.date}>
                <TooltipTrigger asChild>
                  <button
                    className="plugin-activity-cell"
                    data-intensity={intensity}
                    type="button"
                    role="gridcell"
                    aria-label={cell.label}
                    tabIndex={intensity === 0 ? -1 : 0}
                  />
                </TooltipTrigger>
                <TooltipContent side="top">{cell.label}</TooltipContent>
              </Tooltip>
            );
          })}
        </div>
        {dailyLayout ? <div className="plugin-activity-day-labels" aria-hidden="true" style={{ gridTemplateColumns: columns }}>
          {props.cells.map((cell) => <span key={cell.date}>{Number(cell.date.slice(-2))}</span>)}
        </div> : null}
      </div>
    </div>
  );
}

// TimeSeriesPoint describes one labeled numeric chart point.
// TimeSeriesPoint 描述一个带标签数值图点。
export interface TimeSeriesPoint {
  // Stable x-axis key such as an ISO date.
  // 稳定横轴键，例如 ISO 日期。
  key: string;
  // Numeric metric value.
  // 数值指标值。
  value: number;
}

// TimeSeriesSeries describes one colored line and its points.
// TimeSeriesSeries 描述一条彩色折线及其数据点。
export interface TimeSeriesSeries {
  // Stable series identity.
  // 稳定序列身份。
  id: string;
  // Localized series label.
  // 本地化序列标签。
  label: string;
  // Host-compatible CSS color value.
  // 宿主兼容 CSS 颜色值。
  color: string;
  // Ordered points aligned by key across visible series.
  // 跨可见序列按键对齐的有序数据点。
  points: TimeSeriesPoint[];
}

// TimeSeriesLineChartProps describes one responsive multi-series SVG chart.
// TimeSeriesLineChartProps 描述一个响应式多序列 SVG 图表。
export interface TimeSeriesLineChartProps {
  // Visible line series.
  // 可见折线序列。
  series: TimeSeriesSeries[];
  // Accessible chart label.
  // 可访问图表标签。
  ariaLabel: string;
  // Optional compact value formatter.
  // 可选紧凑数值格式器。
  formatValue?: (value: number) => string;
  // Optional empty-state label.
  // 可选空状态标签。
  emptyLabel?: string;
  // Optional hover delay before a point tooltip opens.
  // 数据点 Tooltip 打开前的可选悬停延迟。
  tooltipDelayMs?: number;
}

// TIME_SERIES_WIDTH defines the chart view-box width.
// TIME_SERIES_WIDTH 定义图表视图框宽度。
const TIME_SERIES_WIDTH = 800;

// TIME_SERIES_HEIGHT defines the chart view-box height.
// TIME_SERIES_HEIGHT 定义图表视图框高度。
const TIME_SERIES_HEIGHT = 260;

// TIME_SERIES_PADDING defines the chart drawing inset.
// TIME_SERIES_PADDING 定义图表绘制内边距。
const TIME_SERIES_PADDING = 24;

// TimeSeriesLineChart renders host-native lines without loading a chart runtime.
// TimeSeriesLineChart 在不加载图表运行时的前提下渲染宿主原生折线。
export function TimeSeriesLineChart(props: TimeSeriesLineChartProps) {
  // visibleSeries excludes empty inputs while retaining zero-valued datasets.
  // visibleSeries 排除空输入，同时保留全零数据集。
  const visibleSeries = props.series.filter((item) => item.points.length > 0);
  if (visibleSeries.length === 0) {
    return <PageEmptyState title={props.emptyLabel ?? "No chart data"} />;
  }
  // pointCount is the maximum aligned x-axis length.
  // pointCount 是最大的对齐横轴长度。
  const pointCount = Math.max(...visibleSeries.map((item) => item.points.length));
  // axisSeries is the deterministic longest series that owns every sampled x-axis index.
  // axisSeries 是拥有全部采样横轴索引的确定性最长序列。
  const axisSeries = visibleSeries.reduce((longest, item) => (
    item.points.length > longest.points.length ? item : longest
  ));
  // maximumValue establishes one shared y-axis scale.
  // maximumValue 建立一个共享纵轴比例。
  const maximumValue = Math.max(1, ...visibleSeries.flatMap((item) => item.points.map((point) => point.value)));
  // drawingWidth is the horizontal area inside chart padding.
  // drawingWidth 是图表内边距中的水平区域。
  const drawingWidth = TIME_SERIES_WIDTH - TIME_SERIES_PADDING * 2;
  // drawingHeight is the vertical area inside chart padding.
  // drawingHeight 是图表内边距中的垂直区域。
  const drawingHeight = TIME_SERIES_HEIGHT - TIME_SERIES_PADDING * 2;
  // pointPosition maps one aligned point into SVG coordinates.
  // pointPosition 将一个对齐数据点映射到 SVG 坐标。
  const pointPosition = (index: number, value: number) => ({
    x: TIME_SERIES_PADDING + (pointCount <= 1 ? 0 : (index / (pointCount - 1)) * drawingWidth),
    y: TIME_SERIES_PADDING + drawingHeight - (Math.max(0, value) / maximumValue) * drawingHeight,
  });
  // xAxisLabelIndexes samples at most five stable labels including both range endpoints.
  // xAxisLabelIndexes 最多采样五个稳定标签，并包含时间范围两端。
  const xAxisLabelIndexes = Array.from(new Set(
    Array.from({ length: Math.min(5, pointCount) }, (_, index) => (
      Math.round((index / Math.max(1, Math.min(5, pointCount) - 1)) * (pointCount - 1))
    )),
  ));
  return (
    <div className="plugin-time-series-chart" role="group" aria-label={props.ariaLabel}>
      <svg viewBox={`0 0 ${TIME_SERIES_WIDTH} ${TIME_SERIES_HEIGHT}`} preserveAspectRatio="xMidYMid meet">
        {[0, 0.25, 0.5, 0.75, 1].map((ratio) => {
          // gridY is one shared horizontal guide position.
          // gridY 是一条共享水平参考线位置。
          const gridY = TIME_SERIES_PADDING + drawingHeight * ratio;
          return <line className="plugin-chart-grid-line" key={ratio} x1={TIME_SERIES_PADDING} y1={gridY} x2={TIME_SERIES_WIDTH - TIME_SERIES_PADDING} y2={gridY} />;
        })}
        {visibleSeries.map((item) => {
          // pathData connects all ordered points in the current series.
          // pathData 连接当前序列中的全部有序数据点。
          const pathData = item.points.map((point, index) => {
            // position is the current point's exact shared-scale SVG coordinate.
            // position 是当前数据点在共享比例下的精确 SVG 坐标。
            const position = pointPosition(index, point.value);
            return `${index === 0 ? "M" : "L"}${position.x.toFixed(2)},${position.y.toFixed(2)}`;
          }).join(" ");
          return (
            <g key={item.id}>
              <path className="plugin-chart-line" d={pathData} stroke={item.color} />
              {item.points.map((point, index) => {
                if (point.value <= 0) {
                  return null;
                }
                // position is reused by the focusable visual marker and tooltip.
                // position 由可聚焦视觉标记与提示共同复用。
                const position = pointPosition(index, point.value);
                // formattedValue is the localized accessible value for this point.
                // formattedValue 是当前数据点的本地化可访问数值。
                const formattedValue = props.formatValue?.(point.value) ?? String(point.value);
                return (
                  <Tooltip delayDuration={props.tooltipDelayMs ?? 0} key={point.key}>
                    <TooltipTrigger asChild>
                      <circle
                        aria-label={`${item.label}, ${point.key}, ${formattedValue}`}
                        className="plugin-chart-point"
                        cx={position.x}
                        cy={position.y}
                        fill={item.color}
                        r="3"
                        tabIndex={0}
                      />
                    </TooltipTrigger>
                    <TooltipContent side="top">{`${item.label} · ${point.key}: ${formattedValue}`}</TooltipContent>
                  </Tooltip>
                );
              })}
            </g>
          );
        })}
        {xAxisLabelIndexes.map((index) => {
          // point is read from the first aligned series because all series share x-axis keys.
          // point 从首个对齐序列读取，因为全部序列共享横轴键。
          const point = axisSeries.points[index];
          // position is the label's horizontal coordinate on the shared x-axis.
          // position 是标签在共享横轴上的水平坐标。
          const position = pointPosition(index, 0);
          return <text className="plugin-chart-axis-label" key={point.key} textAnchor={index === 0 ? "start" : index === pointCount - 1 ? "end" : "middle"} x={position.x} y={TIME_SERIES_HEIGHT - 5}>{point.key.slice(5)}</text>;
        })}
      </svg>
    </div>
  );
}

// ChartLegendItem describes one visible or hidden chart series control.
// ChartLegendItem 描述一个可见或隐藏的图表序列控件。
export interface ChartLegendItem {
  // Stable series identity.
  // 稳定序列身份。
  id: string;
  // Localized series label.
  // 本地化序列标签。
  label: string;
  // CSS color shared with the chart.
  // 与图表共享的 CSS 颜色。
  color: string;
  // Whether the series is currently visible.
  // 序列当前是否可见。
  visible?: boolean;
}

// ChartLegendProps describes one optional interactive series legend.
// ChartLegendProps 描述一个可选交互序列图例。
export interface ChartLegendProps {
  // Ordered legend entries.
  // 有序图例条目。
  items: ChartLegendItem[];
  // Optional visibility toggle callback.
  // 可选可见性切换回调。
  onToggle?: (id: string) => void;
}

// ChartLegend renders stable color labels and optional visibility controls.
// ChartLegend 渲染稳定颜色标签与可选可见性控件。
export function ChartLegend(props: ChartLegendProps) {
  return (
    <div className="plugin-chart-legend">
      {props.items.map((item) => (
        <button
          className="plugin-chart-legend-item"
          data-visible={item.visible !== false}
          aria-pressed={props.onToggle ? item.visible !== false : undefined}
          key={item.id}
          type="button"
          onClick={() => props.onToggle?.(item.id)}
          disabled={!props.onToggle}
        >
          <span className="plugin-chart-legend-swatch" style={{ backgroundColor: item.color }} />
          <span>{item.label}</span>
        </button>
      ))}
    </div>
  );
}
