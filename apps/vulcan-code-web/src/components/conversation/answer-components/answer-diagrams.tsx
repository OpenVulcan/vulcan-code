import React from "react";
import { useI18n } from "@/i18n";
import type { AnswerComponent, AnswerErTable, AnswerFlowNode, AnswerMark } from "./answer-component-model";
import { diamondBoundary, layoutGraph, measureText, splinePath, wrapText, type GraphPoint } from "./answer-graph-layout";

type FlowComponent = Extract<AnswerComponent, { kind: "flow" }>;
type ErComponent = Extract<AnswerComponent, { kind: "er" }>;

// Diagram typography in CSS pixels; the stylesheet uses the same sizes.
// 图形排版尺寸（CSS 像素）；样式表使用相同尺寸。
const NODE_FONT_SIZE = 13;
const LABEL_FONT_SIZE = 11.5;
const LINE_HEIGHT = 17;
const NODE_MAX_TEXT_WIDTH = 190;
const LABEL_MAX_TEXT_WIDTH = 150;

// Font family the diagrams inherit from the conversation.
// 图形从会话继承的字体族。
function diagramFont(size: number, weight = 400): string {
  const family = typeof document === "undefined" ? "sans-serif" : getComputedStyle(document.body).fontFamily || "sans-serif";
  return `${weight} ${size}px ${family}`;
}

/** A flow laid out as a layered graph: one node per label, one edge per hop. 以分层图布局的流程：每个标签一个节点，每一跳一条连线。 */
export function AnswerFlowDiagram({ flow }: { flow: FlowComponent }) {
  const id = React.useId().replace(/:/gu, "");
  const drawing = React.useMemo(() => flowDrawing(flow), [flow]);
  const marks = new Set([...flow.nodes.map((node) => node.mark), ...flow.chains.map((chain) => chain.mark)].filter(Boolean) as AnswerMark[]);
  return (
    <DiagramFrame marks={marks} width={drawing.width}>
      <svg className="answer-graph" height={drawing.height} role="img" viewBox={`0 0 ${drawing.width} ${drawing.height}`} width={drawing.width}>
        <ArrowMarkers id={id} />
        {drawing.edges.map((edge, index) => (
          <path className="answer-graph-edge" d={edge.path} data-dashed={edge.dashed || undefined} data-mark={edge.mark} key={index}
            markerEnd={`url(#${id}-${edge.mark ?? "plain"})`} />
        ))}
        {drawing.edges.map((edge, index) => edge.label ? (
          <g className="answer-graph-label" data-mark={edge.mark} key={index} transform={`translate(${edge.label.at.x} ${edge.label.at.y})`}>
            <rect height={edge.label.height} rx={4} width={edge.label.width} x={-edge.label.width / 2} y={-edge.label.height / 2} />
            <TextLines lines={edge.label.lines} />
          </g>
        ) : null)}
        {drawing.nodes.map((node, index) => (
          <g className="answer-graph-node" data-mark={node.mark} data-shape={node.shape} key={index} transform={`translate(${node.at.x} ${node.at.y})`}>
            <NodeShape height={node.height} shape={node.shape} width={node.width} />
            <TextLines lines={node.lines} />
          </g>
        ))}
      </svg>
    </DiagramFrame>
  );
}

/** Tables as cards, references drawn from the field row to the referenced table. 表以卡片显示，引用从字段行连到被引用表。 */
export function AnswerErDiagram({ er }: { er: ErComponent }) {
  const id = React.useId().replace(/:/gu, "");
  const drawing = React.useMemo(() => erDrawing(er), [er]);
  const marks = new Set([...er.tables.map((table) => table.mark), ...er.tables.flatMap((table) => table.fields.map((field) => field.mark))]
    .filter(Boolean) as AnswerMark[]);
  return (
    <DiagramFrame marks={marks} width={drawing.width}>
      <svg className="answer-graph answer-er" height={drawing.height} role="img" viewBox={`0 0 ${drawing.width} ${drawing.height}`} width={drawing.width}>
        <defs>
          <marker id={`${id}-many`} markerHeight={12} markerUnits="userSpaceOnUse" markerWidth={12} orient="auto-start-reverse" refX={12} refY={6} viewBox="0 0 12 12">
            <path className="answer-er-foot" d="M0,6 L12,0 M0,6 L12,6 M0,6 L12,12" />
          </marker>
          <marker id={`${id}-one`} markerHeight={12} markerUnits="userSpaceOnUse" markerWidth={12} orient="auto-start-reverse" refX={12} refY={6} viewBox="0 0 12 12">
            <path className="answer-er-foot" d="M6,0 L6,12 M0,6 L12,6" />
          </marker>
        </defs>
        {drawing.links.map((link, index) => (
          <path className="answer-graph-edge" d={link.path} data-mark={link.mark} key={index}
            markerEnd={`url(#${id}-one)`} markerStart={`url(#${id}-many)`} />
        ))}
        {drawing.tables.map((table, index) => <ErTable key={index} table={table} />)}
      </svg>
    </DiagramFrame>
  );
}

// Diagram frame: an optional change legend and a horizontally scrollable drawing.
// 图形外框：可选的变更图例与可横向滚动的绘图区。
function DiagramFrame({ children, marks, width }: { children: React.ReactNode; marks: Set<AnswerMark>; width: number }) {
  const { t } = useI18n();
  return (
    <figure className="vk-figure answer-diagram">
      {marks.size > 0 ? (
        <figcaption className="answer-diagram-legend">
          {(["add", "chg", "del"] as const).filter((mark) => marks.has(mark)).map((mark) => (
            <span data-mark={mark} key={mark}><i aria-hidden="true" />{t(MARK_KEYS[mark])}</span>
          ))}
        </figcaption>
      ) : null}
      <div className="answer-diagram-scroll" style={{ "--answer-diagram-width": `${width}px` } as React.CSSProperties}>{children}</div>
    </figure>
  );
}

// Catalog keys of plan markers.
// 计划标记的目录键。
const MARK_KEYS = {
  add: "answerComponents.mark.added",
  chg: "answerComponents.mark.changed",
  del: "answerComponents.mark.removed",
} as const;

// Arrowheads per change tone; each instance owns its ids.
// 各变更色调的箭头；每个实例拥有自己的标识。
function ArrowMarkers({ id }: { id: string }) {
  return (
    <defs>
      {(["plain", "add", "chg", "del"] as const).map((mark) => (
        <marker data-mark={mark} id={`${id}-${mark}`} key={mark} markerHeight={8} markerUnits="userSpaceOnUse" markerWidth={10}
          orient="auto-start-reverse" refX={10} refY={4} viewBox="0 0 10 8">
          <path className="answer-graph-arrow" d="M0,0 L10,4 L0,8 z" />
        </marker>
      ))}
    </defs>
  );
}

// Centered text lines around the group origin.
// 围绕分组原点居中的多行文字。
function TextLines({ lines }: { lines: string[] }) {
  return (
    <text textAnchor="middle">
      {lines.map((line, index) => (
        <tspan dominantBaseline="central" key={index} x={0} y={(index - (lines.length - 1) / 2) * LINE_HEIGHT}>{line}</tspan>
      ))}
    </text>
  );
}

// Outline of a flow node centered on the group origin.
// 以分组原点为中心的流程节点轮廓。
function NodeShape({ height, shape, width }: { height: number; shape: AnswerFlowNode["shape"]; width: number }) {
  const [x, y] = [-width / 2, -height / 2];
  switch (shape) {
    case "decision":
      return <polygon points={`0,${y} ${-x},0 0,${-y} ${x},0`} />;
    case "terminal":
      return <rect height={height} rx={height / 2} width={width} x={x} y={y} />;
    case "store": {
      const cap = 5;
      return (
        <>
          <path d={`M${x},${y + cap} V${-y - cap} A${-x},${cap} 0 0 0 ${-x},${-y - cap} V${y + cap}`} />
          <ellipse cx={0} cy={y + cap} rx={-x} ry={cap} />
        </>
      );
    }
    default:
      return <rect height={height} rx={6} width={width} x={x} y={y} />;
  }
}

// One placed flow node.
// 单个已布局的流程节点。
interface PlacedNode {
  shape: AnswerFlowNode["shape"];
  mark?: AnswerMark;
  lines: string[];
  width: number;
  height: number;
  at: GraphPoint;
}

// One routed flow edge.
// 单条已布线的流程连线。
interface RoutedEdge {
  path: string;
  dashed: boolean;
  mark?: AnswerMark;
  label?: { lines: string[]; width: number; height: number; at: GraphPoint };
}

// Measure, lay out and route a flow.
// 测量、布局并布线一个流程。
function flowDrawing(flow: FlowComponent): { width: number; height: number; nodes: PlacedNode[]; edges: RoutedEdge[] } {
  const nodeFont = diagramFont(NODE_FONT_SIZE, 500);
  const labelFont = diagramFont(LABEL_FONT_SIZE);
  const measureNode = (text: string) => measureText(text, nodeFont, NODE_FONT_SIZE);
  const measureLabel = (text: string) => measureText(text, labelFont, LABEL_FONT_SIZE);
  const sized = flow.nodes.map((node) => {
    const lines = wrapText(node.label, NODE_MAX_TEXT_WIDTH, measureNode);
    const textWidth = Math.max(...lines.map(measureNode));
    const textHeight = lines.length * LINE_HEIGHT;
    let width = Math.max(textWidth + 28, 56);
    let height = textHeight + 16;
    if (node.shape === "terminal") width += height / 2;
    if (node.shape === "store") height += 10;
    if (node.shape === "decision") {
      // The diamond is the smallest one whose sides clear the text box.
      // 菱形取能容纳文字框的最小尺寸。
      const halfHeight = textHeight / 2 + 16;
      const halfWidth = (textWidth / 2 + 8) / (1 - (textHeight / 2 + 2) / halfHeight);
      width = halfWidth * 2;
      height = halfHeight * 2;
    }
    return { ...node, lines, width: Math.ceil(width), height: Math.ceil(height) };
  });
  // One edge per hop and node pair; a repeated hop keeps its first label.
  // 每一跳每对节点一条连线；重复的跳保留首个标签。
  const hops = new Map<string, { from: number; to: number; dashed: boolean; mark?: AnswerMark; label?: string }>();
  for (const chain of flow.chains) {
    chain.groups.slice(1).forEach((group, index) => {
      const label = index === chain.groups.length - 2 ? chain.label : undefined;
      for (const from of chain.groups[index]) {
        for (const to of group) {
          const key = `${from}>${to}`;
          const known = hops.get(key);
          if (!known) hops.set(key, { from, to, dashed: chain.dashed[index], mark: chain.mark, label });
          else if (!known.label && label) known.label = label;
        }
      }
    });
  }
  const edges = [...hops.values()].map((edge) => {
    if (!edge.label) return { ...edge, lines: [] as string[], labelWidth: 0, labelHeight: 0 };
    const lines = wrapText(edge.label, LABEL_MAX_TEXT_WIDTH, measureLabel);
    return { ...edge, lines, labelWidth: Math.ceil(Math.max(...lines.map(measureLabel)) + 10), labelHeight: lines.length * 15 + 4 };
  });
  const horizontal = flow.horizontal;
  const placed = layoutGraph(
    sized.map((node, index) => ({ id: String(index), width: node.width, height: node.height })),
    edges.map((edge) => ({ from: String(edge.from), to: String(edge.to), labelWidth: edge.labelWidth, labelHeight: edge.labelHeight })),
    { horizontal, nodeGap: horizontal ? 22 : 30, rankGap: horizontal ? 46 : 34 },
  );
  const nodes = sized.map((node, index) => ({ shape: node.shape, mark: node.mark, lines: node.lines, width: node.width, height: node.height, at: placed.nodes.get(String(index))! }));
  const routed = edges.map((edge, index) => {
    const points = placed.edges[index].points;
    // dagre clips edges at the bounding box; diamonds clip at their sides instead.
    // dagre 在外接矩形处截断连线；菱形改为在其边上截断。
    const source = nodes[edge.from];
    const target = nodes[edge.to];
    if (source.shape === "decision" && points.length > 1) points[0] = diamondBoundary(source.at, source.width / 2, source.height / 2, points[1]);
    if (target.shape === "decision" && points.length > 1) {
      points[points.length - 1] = diamondBoundary(target.at, target.width / 2, target.height / 2, points[points.length - 2]);
    }
    const label = edge.labelWidth && placed.edges[index].label
      ? { lines: edge.lines, width: edge.labelWidth, height: edge.labelHeight, at: placed.edges[index].label! }
      : undefined;
    return { path: splinePath(points), dashed: edge.dashed, mark: edge.mark, label };
  });
  return { width: placed.width, height: placed.height, nodes, edges: routed };
}

// Entity card geometry in CSS pixels.
// 实体卡片几何尺寸（CSS 像素）。
const ER_HEADER_HEIGHT = 30;
const ER_ROW_HEIGHT = 24;
const ER_PADDING = 10;

// One placed entity card with its rows.
// 单个已布局的实体卡片及其行。
interface PlacedTable {
  table: AnswerErTable;
  width: number;
  height: number;
  at: GraphPoint;
}

// Measure, lay out and connect the entity cards.
// 测量、布局并连接实体卡片。
function erDrawing(er: ErComponent) {
  const titleFont = diagramFont(NODE_FONT_SIZE, 600);
  const rowFont = diagramFont(12.5);
  const smallFont = diagramFont(LABEL_FONT_SIZE);
  const sized = er.tables.map((table) => {
    const noteWidth = (note?: string) => note ? measureText(note, smallFont, LABEL_FONT_SIZE) + 8 : 0;
    const rowWidth = Math.max(0, ...table.fields.map((field) => (
      measureText(field.name, rowFont, 12.5) + field.keys.length * 26 + noteWidth(field.note)
      + (field.type ? measureText(field.type, smallFont, LABEL_FONT_SIZE) + 18 : 0)
    )));
    const titleWidth = measureText(table.name, titleFont, NODE_FONT_SIZE) + noteWidth(table.note);
    const width = Math.ceil(Math.max(titleWidth, rowWidth, 100) + ER_PADDING * 2);
    const height = ER_HEADER_HEIGHT + Math.max(table.fields.length, 1) * ER_ROW_HEIGHT;
    return { table, width, height };
  });
  // Layout edges run from the referenced table to the referencing one, so parents come first.
  // 布局连线从被引用表指向引用表，使父表排在前面。
  const references = er.tables.flatMap((table, index) => table.fields.flatMap((field, row) => (
    field.target === undefined ? [] : [{ from: index, row, to: field.target, mark: field.mark ?? table.mark }]
  )));
  const placed = layoutGraph(
    sized.map((table, index) => ({ id: String(index), width: table.width, height: table.height })),
    references.map((reference) => ({ from: String(reference.to), to: String(reference.from) })),
    { horizontal: er.horizontal, nodeGap: 26, rankGap: er.horizontal ? 70 : 46 },
  );
  const tables: PlacedTable[] = sized.map((table, index) => ({ ...table, at: placed.nodes.get(String(index))! }));
  const links = references.map((reference) => {
    const source = tables[reference.from];
    const target = tables[reference.to];
    const rowY = source.at.y - source.height / 2 + ER_HEADER_HEIGHT + (reference.row + 0.5) * ER_ROW_HEIGHT;
    const targetY = target.at.y - target.height / 2 + ER_HEADER_HEIGHT / 2;
    if (reference.from === reference.to) {
      // A self reference loops out of the right side back into the header.
      // 自引用从右侧绕出后回到表头。
      const right = source.at.x + source.width / 2;
      return { mark: reference.mark, path: `M${right},${rowY} C${right + 36},${rowY} ${right + 36},${targetY} ${right},${targetY}` };
    }
    const rightward = target.at.x >= source.at.x;
    const startX = source.at.x + (rightward ? source.width / 2 : -source.width / 2);
    const endX = target.at.x + (rightward ? -target.width / 2 : target.width / 2);
    const bend = Math.max(Math.abs(endX - startX) / 2, 28) * (rightward ? 1 : -1);
    return { mark: reference.mark, path: `M${startX},${rowY} C${startX + bend},${rowY} ${endX - bend},${targetY} ${endX},${targetY}` };
  });
  return { width: placed.width + 40, height: placed.height, tables, links };
}

// One entity card: name header, then one row per field with keys, name and type.
// 单个实体卡片：表名标题，之后每个字段一行，含键、名称与类型。
function ErTable({ table }: { table: PlacedTable }) {
  const x = table.at.x - table.width / 2;
  const y = table.at.y - table.height / 2;
  return (
    <g className="answer-er-table" data-mark={table.table.mark} transform={`translate(${x} ${y})`}>
      <rect className="answer-er-card" height={table.height} rx={7} width={table.width} />
      <path className="answer-er-head" d={`M0,7 a7,7 0 0 1 7,-7 h${table.width - 14} a7,7 0 0 1 7,7 v${ER_HEADER_HEIGHT - 7} h${-table.width} z`} />
      <text dominantBaseline="central" x={ER_PADDING} y={ER_HEADER_HEIGHT / 2}>
        <tspan className="answer-er-title">{table.table.name}</tspan>
        {table.table.note ? <tspan className="answer-er-note" dx={8}>{table.table.note}</tspan> : null}
      </text>
      {table.table.fields.map((field, row) => {
        const rowY = ER_HEADER_HEIGHT + row * ER_ROW_HEIGHT;
        let keyX = ER_PADDING;
        return (
          <g className="answer-er-row" data-mark={field.mark} key={row}>
            {row > 0 ? <line className="answer-er-rule" x1={0} x2={table.width} y1={rowY} y2={rowY} /> : null}
            {field.keys.map((key) => {
              const at = keyX;
              keyX += 26;
              return (
                <g className="answer-er-key" data-key={key} key={key} transform={`translate(${at} ${rowY + ER_ROW_HEIGHT / 2})`}>
                  <rect height={15} rx={3} width={22} y={-7.5} />
                  <text dominantBaseline="central" textAnchor="middle" x={11}>{key}</text>
                </g>
              );
            })}
            <text dominantBaseline="central" x={keyX} y={rowY + ER_ROW_HEIGHT / 2}>
              <tspan className="answer-er-field">{field.name}</tspan>
              {field.note ? <tspan className="answer-er-note" dx={8}>{field.note}</tspan> : null}
            </text>
            {field.type ? <text className="answer-er-type" dominantBaseline="central" textAnchor="end" x={table.width - ER_PADDING} y={rowY + ER_ROW_HEIGHT / 2}>{field.type}</text> : null}
          </g>
        );
      })}
    </g>
  );
}
