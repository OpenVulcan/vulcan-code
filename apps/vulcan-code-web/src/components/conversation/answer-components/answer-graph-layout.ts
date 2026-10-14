import { layout } from "dagre-d3-es/src/dagre/index.js";
import { Graph } from "dagre-d3-es/src/graphlib/index.js";

// Layered graph layout shared by `ui-flow` and `ui-er`: dagre ranks and orders the nodes, this module
// measures labels and turns dagre's edge points into smooth SVG paths.
// `ui-flow` 与 `ui-er` 共用的分层图布局：dagre 负责分层与排序，本模块测量标签并把 dagre 的连线点转为平滑 SVG 路径。

/** A point in diagram coordinates. 图坐标中的点。 */
export interface GraphPoint {
  x: number;
  y: number;
}

/** A node to place: its outer size and an id the caller maps back. 待布局节点：外框尺寸及调用方回查的标识。 */
export interface GraphLayoutNode {
  id: string;
  width: number;
  height: number;
}

/** An edge to route, with the size reserved for its label. 待布线的连线及为标签预留的尺寸。 */
export interface GraphLayoutEdge {
  from: string;
  to: string;
  labelWidth?: number;
  labelHeight?: number;
}

/** Placed graph: node centers, edge points, label centers and the drawing size. 布局结果：节点中心、连线点、标签中心与绘图尺寸。 */
export interface GraphLayout {
  width: number;
  height: number;
  nodes: Map<string, GraphPoint>;
  edges: { points: GraphPoint[]; label?: GraphPoint }[];
}

/** Spacing of one layout direction. 单个布局方向的间距。 */
export interface GraphSpacing {
  horizontal: boolean;
  nodeGap: number;
  rankGap: number;
}

// Margin around the drawing so strokes and arrowheads are never clipped.
// 绘图四周的边距，避免描边与箭头被裁切。
const GRAPH_MARGIN = 6;

/** Rank, order and route the graph; edges keep the input order. 分层、排序并布线；连线保持输入顺序。 */
export function layoutGraph(nodes: GraphLayoutNode[], edges: GraphLayoutEdge[], spacing: GraphSpacing): GraphLayout {
  const graph = new Graph({ multigraph: true });
  graph.setGraph({
    rankdir: spacing.horizontal ? "LR" : "TB",
    nodesep: spacing.nodeGap,
    ranksep: spacing.rankGap,
    edgesep: 12,
    marginx: GRAPH_MARGIN,
    marginy: GRAPH_MARGIN,
  });
  graph.setDefaultEdgeLabel(() => ({}));
  for (const node of nodes) graph.setNode(node.id, { width: node.width, height: node.height });
  edges.forEach((edge, index) => {
    graph.setEdge(edge.from, edge.to, {
      width: edge.labelWidth ?? 0,
      height: edge.labelHeight ?? 0,
      labelpos: "c",
    }, String(index));
  });
  layout(graph, {});
  const placed = new Map<string, GraphPoint>();
  for (const node of nodes) {
    const value = graph.node(node.id) as GraphPoint;
    placed.set(node.id, { x: value.x, y: value.y });
  }
  const routed = edges.map((edge, index) => {
    const value = graph.edge({ v: edge.from, w: edge.to, name: String(index) }) as { points: GraphPoint[]; x?: number; y?: number };
    const label = edge.labelWidth && value.x !== undefined && value.y !== undefined ? { x: value.x, y: value.y } : undefined;
    return { points: value.points.map((point) => ({ x: point.x, y: point.y })), label };
  });
  const size = graph.graph() as { width?: number; height?: number };
  return { width: Math.ceil(size.width ?? 0), height: Math.ceil(size.height ?? 0), nodes: placed, edges: routed };
}

/** Smooth path through `points` (uniform B-spline, as d3's curveBasis). 穿过 `points` 的平滑路径（均匀 B 样条，同 d3 curveBasis）。 */
export function splinePath(points: GraphPoint[]): string {
  const format = (value: number) => Number(value.toFixed(1));
  if (points.length === 0) return "";
  if (points.length < 3) return points.map((point, index) => `${index ? "L" : "M"}${format(point.x)},${format(point.y)}`).join(" ");
  const parts = [`M${format(points[0].x)},${format(points[0].y)}`];
  let [x0, y0, x1, y1] = [points[0].x, points[0].y, points[1].x, points[1].y];
  parts.push(`L${format((5 * x0 + x1) / 6)},${format((5 * y0 + y1) / 6)}`);
  const curve = (x: number, y: number) => {
    parts.push(`C${format((2 * x0 + x1) / 3)},${format((2 * y0 + y1) / 3)} ${format((x0 + 2 * x1) / 3)},${format((y0 + 2 * y1) / 3)} ${format((x0 + 4 * x1 + x) / 6)},${format((y0 + 4 * y1 + y) / 6)}`);
  };
  for (const point of points.slice(2)) {
    curve(point.x, point.y);
    [x0, y0, x1, y1] = [x1, y1, point.x, point.y];
  }
  curve(x1, y1);
  parts.push(`L${format(x1)},${format(y1)}`);
  return parts.join(" ");
}

/** Where the segment from `outside` toward `center` crosses a diamond of half sizes `rx`, `ry`.
 * 从 `outside` 指向 `center` 的线段与半宽 `rx`、半高 `ry` 菱形的交点。 */
export function diamondBoundary(center: GraphPoint, rx: number, ry: number, outside: GraphPoint): GraphPoint {
  const dx = outside.x - center.x;
  const dy = outside.y - center.y;
  const scale = 1 / (Math.abs(dx) / rx + Math.abs(dy) / ry || 1);
  return { x: center.x + dx * scale, y: center.y + dy * scale };
}

// Canvas context used to measure label text, created on first use.
// 用于测量标签文字的画布上下文，首次使用时创建。
let measureContext: CanvasRenderingContext2D | null | undefined;

/** Width of `text` in `font`; estimated when no canvas is available (tests, prerender).
 * `text` 在 `font` 下的宽度；无画布可用时（测试、预渲染）按字符估算。 */
export function measureText(text: string, font: string, fontSize: number): number {
  if (measureContext === undefined) {
    measureContext = typeof document === "undefined" || /jsdom/u.test(navigator.userAgent)
      ? null
      : document.createElement("canvas").getContext("2d");
  }
  if (measureContext) {
    measureContext.font = font;
    return measureContext.measureText(text).width;
  }
  return [...text].reduce((width, ch) => width + (/[⺀-￯]/u.test(ch) ? fontSize : fontSize * 0.56), 0);
}

/** Break `text` into lines no wider than `maxWidth`: at spaces, or between characters of CJK text.
 * 将 `text` 拆为不超过 `maxWidth` 的多行：在空格处断行，CJK 文字可在字符间断行。 */
export function wrapText(text: string, maxWidth: number, measure: (text: string) => number): string[] {
  if (measure(text) <= maxWidth) return [text];
  const tokens = text.match(/[⺀-￯]|[^\s⺀-￯]+|\s+/gu) ?? [text];
  const lines: string[] = [];
  let line = "";
  for (const token of tokens) {
    const next = line + token;
    if (line.trim() && measure(next.trimEnd()) > maxWidth) {
      lines.push(line.trim());
      line = token.trimStart();
    } else {
      line = next;
    }
  }
  if (line.trim()) lines.push(line.trim());
  return lines;
}
