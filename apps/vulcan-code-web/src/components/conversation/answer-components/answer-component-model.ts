import { ANSWER_CHART_MAX_SERIES, ANSWER_COMPONENT_MAX_ROWS } from "@/generated/protocol-limits";

// Parsed `ui-*` answer component fences. Parsing is pure and total: any fence that does not match
// its documented syntax returns null so the caller keeps showing it as ordinary code. The syntax
// authority is `agent-prompts/answer-mode-interactive.md`; the TUI reads the same rules.
// 已解析的 `ui-*` 回答组件围栏。解析为纯函数且不抛错：不符合文档语法的围栏返回 null，调用方继续按普通代码显示。
// 语法权威是 `agent-prompts/answer-mode-interactive.md`；TUI 读取同一套规则。

// Tone words that may start a cell.
// 可出现在单元格开头的色调词。
export type AnswerTone = "ok" | "warn" | "no";

// Checklist status: a tone or an open item.
// 清单状态：色调或待办项。
export type AnswerStatus = AnswerTone | "todo";

// Finding severity.
// 发现项严重度。
export type AnswerSeverity = "high" | "medium" | "low" | "info";

// Plan step status.
// 计划步骤状态。
export type AnswerStepStatus = "done" | "now" | "todo";

// Callout tones.
// 提示框色调。
export type AnswerCalloutTone = "info" | "success" | "warning" | "danger";

// Plan marker on a tree node, flow node, flow chain or sequence message.
// 树节点、流程节点、流程链或时序消息上的计划标记。
export type AnswerMark = "add" | "del" | "chg";

// A cell that may start with a tone word; `text` is empty when the cell is only the tone word.
// 可能以色调词开头的单元格；单元格仅为色调词时 `text` 为空。
export interface AnswerTonedText {
  tone?: AnswerTone;
  text: string;
}

// One metric; `maximum` enables a meter.
// 单个指标；提供 `maximum` 时显示进度条。
export interface AnswerMetric {
  label: string;
  value: string;
  numeric?: number;
  maximum?: number;
  unit?: string;
  tone?: AnswerTone;
}

// One tree node with its nested children.
// 单个树节点及其嵌套子节点。
export interface AnswerTreeNode {
  label: string;
  note?: string;
  mark?: AnswerMark;
  children: AnswerTreeNode[];
}

// One changed file; `added` and `removed` are present together.
// 单个变更文件；`added` 与 `removed` 同时存在。
export interface AnswerFile {
  path: string;
  added?: number;
  removed?: number;
  note?: string;
}

// Chart layouts accepted after `ui-chart`.
// `ui-chart` 之后接受的图表布局。
export type AnswerChartKind = "bar" | "hbar" | "stack" | "line" | "area" | "pie" | "donut";

// Chart data: series names and one row of values per category.
// 图表数据：系列名与每个分类的一行数值。
export interface AnswerChart {
  kind: AnswerChartKind;
  title?: string;
  categoryLabel: string;
  series: string[];
  rows: { category: string; values: number[] }[];
}

// Titled Markdown section of `ui-tabs` or `ui-columns`.
// `ui-tabs` 或 `ui-columns` 的带标题 Markdown 分节。
export interface AnswerSection {
  title: string;
  body: string;
}

// Flow node outline.
// 流程节点轮廓。
export type AnswerFlowShape = "box" | "decision" | "terminal" | "store";

// One distinct flow node; the same label is the same node.
// 单个不同的流程节点；相同标签即同一节点。
export interface AnswerFlowNode {
  label: string;
  shape: AnswerFlowShape;
  mark?: AnswerMark;
}

// One source line of a flow: node groups joined by hops; `label` names the last hop only.
// 流程的一行源码：由连线连接的节点组；`label` 仅标注最后一跳。
export interface AnswerFlowChain {
  groups: number[][];
  dashed: boolean[];
  label?: string;
  mark?: AnswerMark;
}

// One sequence row.
// 单个时序行。
export type AnswerSequenceStep =
  | { kind: "phase"; text: string }
  | { kind: "note"; first: number; last: number; text: string }
  | { kind: "message"; from: number; to: number; dashed: boolean; text: string; mark?: AnswerMark };

// Every renderable component.
// 所有可渲染组件。
export type AnswerComponent =
  | { kind: "kv"; rows: { label: string; value: string }[] }
  | { kind: "metrics"; rows: AnswerMetric[] }
  | { kind: "findings"; rows: { severity: AnswerSeverity; text: string; location?: string }[] }
  | { kind: "compare"; header: string[]; rows: { criterion: string; cells: AnswerTonedText[] }[] }
  | { kind: "checklist"; items: { status: AnswerStatus; item: string; note?: string }[] }
  | { kind: "steps"; steps: { status: AnswerStepStatus; title: string; text?: string }[] }
  | { kind: "timeline"; rows: { time: string; title: string; text?: string }[] }
  | { kind: "cards"; cards: { title: string; text: string; tag?: AnswerTonedText }[] }
  | { kind: "files"; files: AnswerFile[]; added: number; removed: number }
  | { kind: "gallery"; items: { path: string; caption?: string }[] }
  | { kind: "tree"; nodes: AnswerTreeNode[] }
  | { kind: "chart"; chart: AnswerChart }
  | { kind: "flow"; horizontal: boolean; nodes: AnswerFlowNode[]; chains: AnswerFlowChain[] }
  | { kind: "sequence"; numbered: boolean; actors: string[]; steps: AnswerSequenceStep[] }
  | { kind: "callout"; tone: AnswerCalloutTone; title?: string; body: string }
  | { kind: "details"; title: string; body: string }
  | { kind: "tabs" | "columns"; sections: AnswerSection[] };

const TONES = new Set<string>(["ok", "warn", "no"]);
const STATUSES = new Set<string>(["ok", "warn", "no", "todo"]);
const SEVERITIES = new Set<string>(["high", "medium", "low", "info"]);
const STEP_STATUSES = new Set<string>(["done", "now", "todo"]);
const CALLOUT_TONES = new Set<string>(["info", "success", "warning", "danger"]);
const CHART_KINDS = new Set<string>(["bar", "hbar", "stack", "line", "area", "pie", "donut"]);

// Finite decimal number syntax shared by metrics and charts.
// 指标与图表共用的有限十进制数语法。
const NUMBER = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/iu;

// Whether a fence language names an answer component.
// 围栏语言是否命名回答组件。
export function isAnswerComponentLanguage(language: string | null | undefined): boolean {
  return Boolean(language?.startsWith("ui-"));
}

/**
 * Parse one fence; `language` is `ui-<name>`, `meta` the rest of the info string, `body` the text.
 * 解析单个围栏；`language` 为 `ui-<名称>`，`meta` 为信息串其余部分，`body` 为正文。
 * Returns null for unknown names or invalid syntax. 未知名称或语法无效时返回 null。
 */
export function parseAnswerComponent(language: string, meta: string, body: string): AnswerComponent | null {
  const args = meta.trim();
  if (!body.trim()) return null;
  switch (language) {
    case "ui-kv":
      return noArgs(args, () => mapRows(body, 2, 2, ([label, value]) => ({ label, value }), (rows) => ({ kind: "kv", rows })));
    case "ui-metrics":
      return noArgs(args, () => mapRows(body, 2, 5, metric, (rows) => ({ kind: "metrics", rows })));
    case "ui-findings":
      return noArgs(args, () => mapRows(body, 2, 3, ([severity, text, location]) => (
        SEVERITIES.has(severity) ? { severity: severity as AnswerSeverity, text, location } : null
      ), (rows) => ({ kind: "findings", rows })));
    case "ui-compare":
      return noArgs(args, () => compare(body));
    case "ui-checklist":
      return noArgs(args, () => mapRows(body, 2, 3, ([status, item, note]) => (
        STATUSES.has(status) ? { status: status as AnswerStatus, item, note } : null
      ), (items) => ({ kind: "checklist", items })));
    case "ui-steps":
      return noArgs(args, () => mapRows(body, 2, 3, ([status, title, text]) => (
        STEP_STATUSES.has(status) ? { status: status as AnswerStepStatus, title, text } : null
      ), (steps) => ({ kind: "steps", steps })));
    case "ui-timeline":
      return noArgs(args, () => mapRows(body, 2, 3, ([time, title, text]) => ({ time, title, text }), (rows) => ({ kind: "timeline", rows })));
    case "ui-cards":
      return noArgs(args, () => mapRows(body, 2, 3, ([title, text, tag]) => ({ title, text, tag: tag === undefined ? undefined : toned(tag) }),
        (cards) => ({ kind: "cards", cards })));
    case "ui-files":
      return noArgs(args, () => files(body));
    case "ui-gallery":
      return noArgs(args, () => mapRows(body, 1, 2, ([path, caption]) => ({ path, caption }), (items) => ({ kind: "gallery", items })));
    case "ui-tree":
      return noArgs(args, () => tree(body));
    case "ui-chart":
      return chart(args, body);
    case "ui-flow":
      return args === "" || args === "LR" ? flow(args === "LR", body) : null;
    case "ui-sequence":
      return args === "" || args === "num" ? sequence(args === "num", body) : null;
    case "ui-callout": {
      const [tone = "", ...title] = args.split(/\s+/u).filter(Boolean);
      if (!CALLOUT_TONES.has(tone)) return null;
      return { kind: "callout", tone: tone as AnswerCalloutTone, title: title.join(" ") || undefined, body };
    }
    case "ui-details":
      return args ? { kind: "details", title: args, body } : null;
    case "ui-tabs":
    case "ui-columns":
      return noArgs(args, () => sections(language === "ui-tabs" ? "tabs" : "columns", body));
    default:
      return null;
  }
}

// Run `parse` only when the fence carries no arguments.
// 仅当围栏不带参数时运行 `parse`。
function noArgs(args: string, parse: () => AnswerComponent | null): AnswerComponent | null {
  return args ? null : parse();
}

// Split one row into trimmed cells; outer pipes are optional.
// 将一行拆为去空白单元格；外侧竖线可省略。
export function answerCells(line: string): string[] {
  let text = line.trim();
  if (text.startsWith("|")) text = text.slice(1);
  if (text.endsWith("|")) text = text.slice(0, -1);
  return text.split("|").map((cell) => cell.trim());
}

// Nonblank lines, skipping a Markdown table delimiter row.
// 非空行，跳过 Markdown 表格分隔行。
function dataLines(body: string): string[] {
  return body.split(/\r?\n/u).filter((line) => line.trim() && !(line.includes("-") && /^[\s|:-]+$/u.test(line)));
}

// Cells of a row with `min..=max` nonempty cells, or null.
// 含 `min..=max` 个非空单元格的行的单元格；否则为 null。
function rowCells(line: string, min: number, max: number): string[] | null {
  const cells = answerCells(line);
  return cells.length >= min && cells.length <= max && cells.every(Boolean) ? cells : null;
}

// Parse rows of `min..=max` nonempty cells (trailing cells optional) and wrap them; null when invalid.
// 解析含 `min..=max` 个非空单元格的行（末尾单元格可省略）并包装；无效时返回 null。
function mapRows<T>(
  body: string,
  min: number,
  max: number,
  row: (cells: string[]) => T | null,
  wrap: (rows: T[]) => AnswerComponent,
): AnswerComponent | null {
  const lines = dataLines(body);
  if (lines.length === 0 || lines.length > ANSWER_COMPONENT_MAX_ROWS) return null;
  const rows: T[] = [];
  for (const line of lines) {
    const cells = rowCells(line, min, max);
    const parsed = cells === null ? null : row(cells);
    if (parsed === null) return null;
    rows.push(parsed);
  }
  return wrap(rows);
}

// Split a leading tone word from a cell.
// 从单元格拆出前导色调词。
function toned(cell: string): AnswerTonedText {
  const [word, ...rest] = cell.split(" ");
  return TONES.has(word) ? { tone: word as AnswerTone, text: rest.join(" ").trim() } : { text: cell };
}

// A finite number, or null.
// 有限数值；否则为 null。
function number(text: string): number | null {
  return NUMBER.test(text) ? Number(text) : null;
}

// `label | value [| maximum [| unit]] [| tone]`; a maximum requires a numeric value within range.
// `标签 | 数值 [| 最大值 [| 单位]] [| 色调]`；提供最大值时数值必须为范围内的数字。
function metric(input: string[]): AnswerMetric | null {
  const cells = [...input];
  const tone = cells.length > 2 && TONES.has(cells[cells.length - 1]) ? cells.pop() as AnswerTone : undefined;
  const [label, value, maximum, unit] = cells;
  if (maximum === undefined) return { label, value, tone };
  const numeric = number(value);
  const max = number(maximum);
  if (numeric === null || max === null || max <= 0 || numeric < 0 || numeric > max) return null;
  return { label, value, numeric, maximum: max, unit, tone };
}

// Decision matrix: a header of criterion and at least two options, then one row per criterion.
// 决策矩阵：首行为评估项与至少两个选项，之后每个评估项一行。
function compare(body: string): AnswerComponent | null {
  const [headerLine, ...lines] = dataLines(body);
  if (!headerLine || lines.length === 0 || lines.length > ANSWER_COMPONENT_MAX_ROWS) return null;
  const header = rowCells(headerLine, 3, Number.MAX_SAFE_INTEGER);
  if (header === null) return null;
  const rows: { criterion: string; cells: AnswerTonedText[] }[] = [];
  for (const line of lines) {
    const cells = rowCells(line, header.length, header.length);
    if (cells === null) return null;
    rows.push({ criterion: cells[0], cells: cells.slice(1).map(toned) });
  }
  return { kind: "compare", header, rows };
}

// `path | +added -removed | note` rows; both later cells are optional, totals are summed.
// `路径 | +新增 -删除 | 说明` 行；后两个单元格可省略，并汇总合计。
function files(body: string): AnswerComponent | null {
  let added = 0;
  let removed = 0;
  return mapRows(body, 1, 3, ([path, second, third]): AnswerFile | null => {
    const delta = second === undefined ? null : /^\+(\d+)\s+[-−](\d+)$/u.exec(second);
    if (delta === null && third !== undefined) return null;
    if (delta === null) return { path, note: second };
    const file = { path, added: Number(delta[1]), removed: Number(delta[2]), note: third };
    added += file.added;
    removed += file.removed;
    return file;
  }, (rows) => ({ kind: "files", files: rows, added, removed }));
}

// Split a leading plan marker (`+ `, `- `, `~ `) from a line; a leading `\` keeps the text literal.
// 从一行拆出前导计划标记（`+ `、`- `、`~ `）；前导 `\` 保留字面文字。
function change(line: string): { mark?: AnswerMark; text: string } {
  if (line.startsWith("\\")) return { text: line.slice(1) };
  const marker = /^([+~-]) /u.exec(line);
  if (!marker) return { text: line };
  const mark = marker[1] === "+" ? "add" : marker[1] === "-" ? "del" : "chg";
  return { mark, text: line.slice(2).trimStart() };
}

// Indentation tree with optional `label | note`; tabs count as four columns.
// 缩进树，可带 `标签 | 说明`；制表符按四列计算。
function tree(body: string): AnswerComponent | null {
  const lines = body.split(/\r?\n/u).filter((line) => line.trim());
  if (lines.length > ANSWER_COMPONENT_MAX_ROWS) return null;
  const roots: AnswerTreeNode[] = [];
  const stack: { indent: number; node: AnswerTreeNode }[] = [];
  for (const line of lines) {
    const indent = [.../^\s*/u.exec(line)![0]].reduce((width, ch) => width + (ch === "\t" ? 4 : 1), 0);
    const { mark, text } = change(line.trim());
    const split = text.indexOf("|");
    const label = (split < 0 ? text : text.slice(0, split)).trim();
    const note = split < 0 ? undefined : text.slice(split + 1).trim() || undefined;
    const node: AnswerTreeNode = { label, note, mark, children: [] };
    while (stack.length > 0 && stack[stack.length - 1].indent >= indent) stack.pop();
    (stack.length > 0 ? stack[stack.length - 1].node.children : roots).push(node);
    stack.push({ indent, node });
  }
  return { kind: "tree", nodes: roots };
}

// `### Title` lines split the body into titled Markdown sections; two or more nonempty sections.
// `### 标题` 行将正文拆为带标题的 Markdown 分节；需要两个及以上非空分节。
function sections(kind: "tabs" | "columns", body: string): AnswerComponent | null {
  const result: AnswerSection[] = [];
  let current: { title: string; lines: string[] } | null = null;
  for (const line of body.split(/\r?\n/u)) {
    if (line.startsWith("### ")) {
      if (current) result.push({ title: current.title, body: current.lines.join("\n").trim() });
      current = { title: line.slice(4).trim(), lines: [] };
    } else if (current) {
      current.lines.push(line);
    } else if (line.trim()) {
      return null;
    }
  }
  if (current) result.push({ title: current.title, body: current.lines.join("\n").trim() });
  if (result.length < 2 || result.length > ANSWER_COMPONENT_MAX_ROWS || result.some((section) => !section.body)) return null;
  return { kind, sections: result };
}

// `ui-chart <kind> [title]`: header row of category and series names, then one number per series.
// `ui-chart <类型> [标题]`：首行为分类名与系列名，之后每行每个系列一个数值。
function chart(args: string, body: string): AnswerComponent | null {
  const [kind = "", ...title] = args.split(/\s+/u).filter(Boolean);
  if (!CHART_KINDS.has(kind)) return null;
  const [headerLine, ...lines] = dataLines(body);
  if (!headerLine || lines.length === 0 || lines.length > ANSWER_COMPONENT_MAX_ROWS) return null;
  const header = rowCells(headerLine, 2, ANSWER_CHART_MAX_SERIES + 1);
  const circular = kind === "pie" || kind === "donut";
  if (header === null || (circular && header.length !== 2)) return null;
  const rows: AnswerChart["rows"] = [];
  for (const line of lines) {
    const cells = rowCells(line, header.length, header.length);
    if (cells === null) return null;
    const values = cells.slice(1).map((cell) => number(cell.replace(/%$/u, "").trim()));
    if (values.some((value) => value === null)) return null;
    rows.push({ category: cells[0], values: values as number[] });
  }
  if ((circular || kind === "stack") && rows.some((row) => row.values.some((value) => value < 0))) return null;
  if (circular && rows.reduce((sum, row) => sum + row.values[0], 0) <= 0) return null;
  return { kind: "chart", chart: { kind: kind as AnswerChartKind, title: title.join(" ") || undefined, categoryLabel: header[0], series: header.slice(1), rows } };
}

// Split at the first ASCII or full-width colon, or a `|` as in row components, into a trimmed head and an optional tail.
// 在第一个 ASCII 或全角冒号，或与行组件相同的 `|` 处拆分为去空白的前段和可选后段。
function labelled(text: string): { head: string; tail?: string } {
  const index = text.search(/[:：|]/u);
  return index < 0 ? { head: text.trim() } : { head: text.slice(0, index).trim(), tail: text.slice(index + 1).trim() };
}

// Flow node outline and label from one node spec: `[(Store)]`, `{Decision}`, `(Start)`, `[Box]` or plain.
// 从单个节点写法得到轮廓与标签：`[(存储)]`、`{判断}`、`(起止)`、`[方框]` 或纯文字。
function flowNode(spec: string): { label: string; shape: AnswerFlowShape } | null {
  const text = spec.trim();
  const outlines: [string, string, AnswerFlowShape][] = [["[(", ")]", "store"], ["{", "}", "decision"], ["(", ")", "terminal"], ["[", "]", "box"]];
  for (const [open, close, shape] of outlines) {
    if (text.length >= open.length + close.length && text.startsWith(open) && text.endsWith(close)) {
      const label = text.slice(open.length, text.length - close.length).trim();
      return label ? { label, shape } : null;
    }
  }
  return text ? { label: text, shape: "box" } : null;
}

// `ui-flow [LR]`: one chain per line; nodes keep their first outline unless a later one is explicit.
// `ui-flow [LR]`：每行一条链；节点保留首个轮廓，除非之后显式指定。
function flow(horizontal: boolean, body: string): AnswerComponent | null {
  const lines = dataLines(body);
  if (lines.length === 0 || lines.length > ANSWER_COMPONENT_MAX_ROWS) return null;
  const nodes: AnswerFlowNode[] = [];
  const ids = new Map<string, number>();
  const chains: AnswerFlowChain[] = [];
  const changed = new Set<number>();
  const node = (spec: string): number | null => {
    const parsed = flowNode(spec);
    if (parsed === null) return null;
    let id = ids.get(parsed.label);
    if (id === undefined) {
      id = nodes.push({ label: parsed.label, shape: "box" }) - 1;
      ids.set(parsed.label, id);
    }
    if (parsed.shape !== "box") nodes[id].shape = parsed.shape;
    return id;
  };
  for (const line of lines) {
    const { mark, text } = change(line.trim());
    const parts: string[] = [];
    const dashed: boolean[] = [];
    let rest = text;
    for (let index = rest.indexOf("->"); index >= 0; index = rest.indexOf("->")) {
      const dash = index > 0 && rest[index - 1] === "-";
      parts.push(rest.slice(0, dash ? index - 1 : index));
      dashed.push(dash);
      rest = rest.slice(index + 2);
    }
    if (parts.length === 0) {
      const id = node(text);
      if (id === null) return null;
      if (mark === "chg") changed.add(id);
      chains.push({ groups: [[id]], dashed: [], mark });
      continue;
    }
    const { head, tail } = labelled(rest);
    parts.push(head);
    const groups: number[][] = [];
    for (const part of parts) {
      const group = part.split("&").map(node);
      if (group.some((id) => id === null)) return null;
      groups.push(group as number[]);
    }
    chains.push({ groups, dashed, label: tail || undefined, mark });
  }
  // Nodes drawn only on added (or only on removed) chains carry that marker; `~ Node` lines mark changes.
  // 仅出现在新增（或仅出现在删除）链上的节点带该标记；`~ 节点` 行标记变更。
  const linked = new Set<number>();
  nodes.forEach((entry, id) => {
    const marks = new Set(chains.filter((chain) => chain.groups.length > 1 && chain.groups.some((group) => group.includes(id))).map((chain) => chain.mark));
    if (marks.size > 0) linked.add(id);
    const [only] = [...marks];
    if (marks.size === 1 && (only === "add" || only === "del")) entry.mark = only;
    if (changed.has(id)) entry.mark = "chg";
  });
  // A lone node line only declares or marks a node that already appears on a chain.
  // 单节点行仅声明或标记已出现在链上的节点。
  const shown = chains.filter((chain) => chain.groups.length > 1 || !linked.has(chain.groups[0][0]));
  return { kind: "flow", horizontal, nodes, chains: shown };
}

// `ui-sequence [num]`: messages, notes and phases; actors appear in first-use order.
// `ui-sequence [num]`：消息、注释与阶段；参与者按首次出现排序。
function sequence(numbered: boolean, body: string): AnswerComponent | null {
  const lines = dataLines(body);
  if (lines.length === 0 || lines.length > ANSWER_COMPONENT_MAX_ROWS) return null;
  const actors: string[] = [];
  const actor = (name: string): number | null => {
    const trimmed = name.trim();
    if (!trimmed) return null;
    const known = actors.indexOf(trimmed);
    return known >= 0 ? known : actors.push(trimmed) - 1;
  };
  const steps: AnswerSequenceStep[] = [];
  for (const raw of lines) {
    const line = raw.trim();
    if (line.length >= 4 && line.startsWith("==") && line.endsWith("==")) {
      steps.push({ kind: "phase", text: line.slice(2, -2).trim() });
      continue;
    }
    if (line.startsWith("note ")) {
      const { head, tail } = labelled(line.slice(5));
      const indexes = head.split(/[,，]/u).map(actor);
      if (tail === undefined || indexes.some((index) => index === null)) return null;
      const span = indexes as number[];
      steps.push({ kind: "note", first: Math.min(...span), last: Math.max(...span), text: tail });
      continue;
    }
    const { mark, text } = change(line);
    const index = text.indexOf("->");
    if (index < 0) return null;
    const dashed = index > 0 && text[index - 1] === "-";
    const from = actor(text.slice(0, dashed ? index - 1 : index));
    const { head, tail } = labelled(text.slice(index + 2));
    const to = actor(head);
    if (from === null || to === null) return null;
    steps.push({ kind: "message", from, to, dashed, text: tail ?? "", mark });
  }
  return actors.length > 0 ? { kind: "sequence", numbered, actors, steps } : null;
}
