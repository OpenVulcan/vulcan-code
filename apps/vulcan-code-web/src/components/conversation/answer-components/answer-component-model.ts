import {
  ANSWER_CHART_MAX_SERIES,
  ANSWER_CODE_MAX_EXCERPTS,
  ANSWER_CODE_MAX_LINES,
  ANSWER_COMPONENT_MAX_ROWS,
} from "@/generated/protocol-limits";
import { answerLocationPath } from "./answer-file-location";

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

// One metric; `maximum` enables a meter, and `over` marks a value above it.
// 单个指标；提供 `maximum` 时显示进度条，`over` 表示数值超出最大值。
export interface AnswerMetric {
  label: string;
  value: string;
  numeric?: number;
  maximum?: number;
  over?: boolean;
  unit?: string;
  tone?: AnswerTone;
}

// Key flags of an entity field.
// 实体字段的键标记。
export type AnswerErKey = "PK" | "FK" | "UK";

// One entity field; `target` indexes the referenced table.
// 单个实体字段；`target` 为被引用表的下标。
export interface AnswerErField {
  name: string;
  type?: string;
  keys: AnswerErKey[];
  target?: number;
  note?: string;
  mark?: AnswerMark;
}

// One entity (table) with its fields.
// 单个实体（表）及其字段。
export interface AnswerErTable {
  name: string;
  note?: string;
  mark?: AnswerMark;
  fields: AnswerErField[];
}

// One symbol of a formula legend.
// 公式图例中的单个符号。
export interface AnswerMathSymbol {
  symbol: string;
  meaning: string;
}

// One option of a decision; `recommended` marks the option the answer would pick.
// 决策的单个选项；`recommended` 标记回答建议的选项。
export interface AnswerDecisionOption {
  label: string;
  note?: string;
  recommended: boolean;
}

// One tree node with its nested children.
// 单个树节点及其嵌套子节点。
export interface AnswerTreeNode {
  label: string;
  note?: string;
  mark?: AnswerMark;
  children: AnswerTreeNode[];
}

// One changed file. Either line count may be missing when the row names only one side (`+145`); `status` is a
// change label such as `new` written in place of the counts.
// 单个变更文件。行只写出一侧（如 `+145`）时另一侧行数缺省；`status` 是写在行数位置的变更标签，如 `new`。
export interface AnswerFile {
  path: string;
  added?: number;
  removed?: number;
  status?: string;
  note?: string;
}

// One `ui-code` excerpt: a quoted file range, the lines to highlight inside it and an optional note.
// `location` is the exact cell, the key of the answer-time capture.
// 单个 `ui-code` 摘录：引用的文件范围、其中需高亮的行与可选说明。`location` 是精确单元格，即回答当时抓取结果的键。
export interface AnswerCodeExcerpt {
  location: string;
  path: string;
  startLine: number;
  endLine: number;
  highlighted: number[];
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

// Every renderable component; row components may carry a heading written after their name.
// 所有可渲染组件；行组件可携带写在名称之后的标题。
export type AnswerComponent = AnswerComponentBody & { heading?: string };

// Every renderable component without its heading.
// 不含标题的所有可渲染组件。
export type AnswerComponentBody =
  | { kind: "kv"; rows: { label: string; value: string }[] }
  | { kind: "metrics"; rows: AnswerMetric[] }
  | { kind: "findings"; rows: { severity: AnswerSeverity; text: string; location?: string; note?: string }[] }
  | { kind: "compare"; header: string[]; rows: { criterion: string; cells: AnswerTonedText[] }[] }
  | { kind: "checklist"; items: { status: AnswerStatus; item: string; note?: string; location?: string }[] }
  | { kind: "steps"; steps: { status: AnswerStepStatus; title: string; text?: string }[] }
  | { kind: "timeline"; rows: { time: string; title: string; text?: string }[] }
  | { kind: "cards"; cards: { title: string; text: string; tag?: AnswerTonedText }[] }
  | { kind: "files"; files: AnswerFile[]; added: number; removed: number }
  | { kind: "code"; excerpts: AnswerCodeExcerpt[] }
  | { kind: "gallery"; items: { path: string; caption?: string }[] }
  | { kind: "tree"; nodes: AnswerTreeNode[] }
  | { kind: "chart"; chart: AnswerChart }
  | { kind: "flow"; horizontal: boolean; nodes: AnswerFlowNode[]; chains: AnswerFlowChain[] }
  | { kind: "sequence"; numbered: boolean; actors: string[]; steps: AnswerSequenceStep[] }
  | { kind: "er"; horizontal: boolean; tables: AnswerErTable[] }
  | { kind: "math"; title?: string; equations: string[]; symbols: AnswerMathSymbol[] }
  | { kind: "decision"; multiple: boolean; question: string; options: AnswerDecisionOption[] }
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
      return titled(args, () => mapRows(body, 2, 2, ([label, value]) => ({ label, value }), (rows) => ({ kind: "kv", rows })));
    case "ui-metrics":
      return titled(args, () => mapLines(body, metricCells, metric, (rows) => ({ kind: "metrics", rows })));
    case "ui-findings":
      return titled(args, () => findings(body));
    case "ui-compare":
      return titled(args, () => compare(body));
    case "ui-checklist":
      return titled(args, () => checklist(body));
    case "ui-steps":
      return titled(args, () => mapRows(body, 2, 3, ([status, title, text]) => (
        STEP_STATUSES.has(status) ? { status: status as AnswerStepStatus, title, text } : null
      ), (steps) => ({ kind: "steps", steps })));
    case "ui-timeline":
      return titled(args, () => mapRows(body, 2, 3, ([time, title, text]) => ({ time, title, text }), (rows) => ({ kind: "timeline", rows })));
    case "ui-cards":
      return titled(args, () => mapRows(body, 2, 3, ([title, text, tag]) => ({ title, text, tag: tag === undefined ? undefined : toned(tag) }),
        (cards) => ({ kind: "cards", cards })));
    case "ui-files":
      return titled(args, () => files(body));
    case "ui-code":
      return titled(args, () => code(body));
    case "ui-gallery":
      return titled(args, () => mapRows(body, 1, 2, ([path, caption]) => ({ path, caption }), (items) => ({ kind: "gallery", items })));
    case "ui-tree":
      return titled(args, () => tree(body));
    case "ui-chart":
      return chart(args, body);
    case "ui-flow":
      return args === "" || args === "LR" ? flow(args === "LR", body) : null;
    case "ui-sequence":
      return args === "" || args === "num" ? sequence(args === "num", body) : null;
    case "ui-er":
      return args === "" || args === "LR" || args === "TB" ? er(args !== "TB", body) : null;
    case "ui-math":
      return math(args, body);
    case "ui-decision":
      return args === "" || args === "multi" ? decision(args === "multi", body) : null;
    case "ui-callout":
      return callout(args, body);
    case "ui-details":
      return args ? { kind: "details", title: args, body } : null;
    case "ui-tabs":
    case "ui-columns":
      return titled(args, () => sections(language === "ui-tabs" ? "tabs" : "columns", body));
    default:
      return null;
  }
}

// Run `parse` for a row component; models often write a title after the name, which becomes its heading.
// 运行行组件的 `parse`；模型常在名称后写标题，该文本作为其标题显示。
function titled(args: string, parse: () => AnswerComponent | null): AnswerComponent | null {
  const component = parse();
  return component && args ? { ...component, heading: args } : component;
}

// Split one row into trimmed cells; outer pipes are optional and `\|` is a literal pipe. `lenient` also keeps
// pipes inside balanced backtick code and pipes touching text on both sides (`fetch|iex`) as literal.
// 将一行拆为去空白单元格；外侧竖线可省略，`\|` 为字面竖线。`lenient` 还把成对反引号代码内的竖线，
// 以及两侧紧贴文字的竖线（`fetch|iex`）视为字面字符。
export function answerCells(line: string, lenient = false): string[] {
  let text = line.trim();
  if (text.startsWith("|")) text = text.slice(1);
  if (text.endsWith("|") && !text.endsWith("\\|")) text = text.slice(0, -1);
  const codeAware = lenient && (text.match(/`/gu)?.length ?? 0) % 2 === 0;
  const cells: string[] = [];
  let cell = "";
  let inCode = false;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    if (char === "\\" && text[index + 1] === "|") {
      cell += "|";
      index += 1;
    } else if (char === "`") {
      if (codeAware) inCode = !inCode;
      cell += char;
    } else if (char === "|" && !(inCode || (lenient && /\S/u.test(text[index - 1] ?? " ") && /\S/u.test(text[index + 1] ?? " ")))) {
      cells.push(cell.trim());
      cell = "";
    } else {
      cell += char;
    }
  }
  cells.push(cell.trim());
  return cells;
}

// Nonblank lines, skipping a Markdown table delimiter row.
// 非空行，跳过 Markdown 表格分隔行。
function dataLines(body: string): string[] {
  return body.split(/\r?\n/u).filter((line) => line.trim() && !(line.includes("-") && /^[\s|:-]+$/u.test(line)));
}

// Cells of a row with `min..=max` nonempty cells, or null. A row with too many cells is split again leniently,
// since models write unescaped pipes inside commands such as `fetch|iex`.
// 含 `min..=max` 个非空单元格的行的单元格；否则为 null。单元格过多的行会宽松地再拆一次，
// 因为模型会在 `fetch|iex` 这类命令中写入未转义的竖线。
function rowCells(line: string, min: number, max: number): string[] | null {
  const valid = (cells: string[]) => cells.length >= min && cells.length <= max && cells.every(Boolean);
  const cells = answerCells(line);
  if (valid(cells)) return cells;
  if (cells.length <= max) return null;
  const lenient = answerCells(line, true);
  return valid(lenient) ? lenient : null;
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
  return mapLines(body, (line) => rowCells(line, min, max), row, wrap);
}

// Parse rows whose cells come from `cells` and wrap them; null when a row is invalid or the row count is out of range.
// 解析单元格由 `cells` 拆出的各行并包装；任一行无效或行数越界时返回 null。
function mapLines<T>(
  body: string,
  cells: (line: string) => string[] | null,
  row: (cells: string[]) => T | null,
  wrap: (rows: T[]) => AnswerComponent,
): AnswerComponent | null {
  const lines = dataLines(body);
  if (lines.length === 0 || lines.length > ANSWER_COMPONENT_MAX_ROWS) return null;
  const rows: T[] = [];
  for (const line of lines) {
    const split = cells(line);
    const parsed = split === null ? null : row(split);
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

// `label | value [| maximum [| unit]] [| tone]`; a maximum requires a nonnegative numeric value, and a value
// above it is an overrun whose tone defaults to `no`. Models also write `label | value | unit`: a lone third cell
// that is not a number is read as the unit, without a meter. Models also leave the maximum and unit cells empty
// to keep the columns aligned (`label | value | | | tone`, `label | value | | MB`); an empty cell in those two
// positions means the value is absent, while an empty label, value or any other cell is still malformed.
// `标签 | 数值 [| 最大值 [| 单位]] [| 色调]`；提供最大值时数值必须为非负数字，超出最大值即为超限，色调默认 `no`。
// 模型也会写 `标签 | 数值 | 单位`：单独的第三格不是数字时读作单位，不显示进度条。
// 模型还会把最大值与单位格留空以对齐列（`标签 | 数值 | | | 色调`、`标签 | 数值 | | MB`）：这两个位置的空格表示该项缺省，
// 标签、数值或其他位置为空仍属格式错误。
function metric(input: string[]): AnswerMetric | null {
  const cells = [...input];
  if (!cells[0] || !cells[1]) return null;
  const tone = cells.length > 2 && TONES.has(cells[cells.length - 1]) ? cells.pop() as AnswerTone : undefined;
  if (cells.slice(4).some((cell) => !cell)) return null;
  const [label, value] = cells;
  const maximum = cells[2] || undefined;
  const unit = cells[3] || undefined;
  if (maximum === undefined) return { label, value, unit, tone };
  const max = number(maximum);
  if (max === null && unit === undefined) return { label, value, unit: maximum, tone };
  const numeric = number(value);
  if (numeric === null || max === null || max <= 0 || numeric < 0) return null;
  const over = numeric > max;
  return { label, value, numeric, maximum: max, over, unit, tone: tone ?? (over ? "no" : undefined) };
}

// Cells of one metric row, 2..=5 of them, keeping empty cells for `metric` to judge; a row with too many cells is
// split again leniently, as `rowCells` does.
// 单条指标行的单元格（2 到 5 个），保留空单元格交给 `metric` 判断；单元格过多的行与 `rowCells` 一样宽松地再拆一次。
function metricCells(line: string): string[] | null {
  const fits = (cells: string[]) => cells.length >= 2 && cells.length <= 5;
  const cells = answerCells(line);
  if (fits(cells)) return cells;
  if (cells.length <= 5) return null;
  const lenient = answerCells(line, true);
  return fits(lenient) ? lenient : null;
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

// `path | +added -removed | note` rows; both later cells are optional, totals are summed. Models also write one
// side only (`+145`), a change label in place of the counts (`path | new | note`), or leave the counts cell empty
// to keep the columns aligned (`path | | note`); all are accepted.
// `路径 | +新增 -删除 | 说明` 行；后两个单元格可省略，并汇总合计。模型也会只写一侧（`+145`），或在行数位置写变更标签
// （`路径 | 新建 | 说明`），或为对齐列把行数格留空（`路径 | | 说明`），均被接受。
function files(body: string): AnswerComponent | null {
  let added = 0;
  let removed = 0;
  return mapLines(body, fileCells, ([path, second, third]): AnswerFile | null => {
    const delta = second === undefined ? null : lineDelta(second);
    if (delta === null && third !== undefined) return { path, status: second, note: third };
    if (delta === null) return { path, note: second };
    added += delta.added ?? 0;
    removed += delta.removed ?? 0;
    return { path, ...delta, note: third };
  }, (rows) => ({ kind: "files", files: rows, added, removed }));
}

// Cells of one `ui-files` row: 1..=3 nonempty cells, or `path | | note` with an empty counts cell, read as
// `path | note`. Any other empty cell still makes the row malformed.
// 单条 `ui-files` 行的单元格：1 到 3 个非空单元格，或行数格为空的 `路径 | | 说明`，按 `路径 | 说明` 读取。
// 其他位置为空仍属格式错误。
function fileCells(line: string): string[] | null {
  const plain = rowCells(line, 1, 3);
  if (plain !== null) return plain;
  const [path, counts, note, ...rest] = answerCells(line);
  return rest.length === 0 && path && counts === "" && note ? [path, note] : null;
}

// Line counts of `+added -removed`, `+added` or `-removed` (`−` also marks removals); null for other text.
// `+新增 -删除`、`+新增` 或 `-删除`（`−` 同样表示删除）的行数；其他文字为 null。
function lineDelta(cell: string): { added?: number; removed?: number } | null {
  const match = /^(?:\+(\d+)(?:\s+[-−](\d+))?|[-−](\d+))$/u.exec(cell);
  if (match === null) return null;
  const removed = match[2] ?? match[3];
  return {
    ...(match[1] === undefined ? {} : { added: Number(match[1]) }),
    ...(removed === undefined ? {} : { removed: Number(removed) }),
  };
}

// `severity | finding | location` rows. Models sometimes add a fourth note cell as in `ui-checklist`; it is read
// only after the documented split, including the lenient one, has failed, so `fetch|iex` stays in the finding.
// `严重度 | 问题 | 位置` 行。模型有时仿照 `ui-checklist` 追加第四格说明；仅当包括宽松拆分在内的
// 文档写法都失败后才按四格读取，因此 `fetch|iex` 仍留在问题文字中。
function findings(body: string): AnswerComponent | null {
  const cells = (line: string) => rowCells(line, 2, 3) ?? rowCells(line, 4, 4);
  return mapLines(body, cells, ([severity, text, location, note]) => (
    SEVERITIES.has(severity) ? { severity: severity as AnswerSeverity, text, location, note } : null
  ), (rows) => ({ kind: "findings", rows }));
}

// Row tone words models reuse as callout tones.
// 模型挪用为提示框色调的行色调词。
const CALLOUT_TONE_ALIASES = new Map<string, AnswerCalloutTone>([["ok", "success"], ["warn", "warning"], ["no", "danger"]]);

// `ui-callout <tone> [title]`. Models sometimes omit the tone or write a row tone word: a row tone word maps to its
// callout tone, and without any tone word the callout is `info` with the whole text after the name as its title.
// `ui-callout <色调> [标题]`。模型有时省略色调或写行色调词：行色调词映射为对应提示框色调；完全没有色调词时
// 提示框为 `info`，名称后的全部文字作为标题。
function callout(args: string, body: string): AnswerComponent {
  const [word = "", ...rest] = args.split(/\s+/u).filter(Boolean);
  const tone = CALLOUT_TONES.has(word) ? word as AnswerCalloutTone : CALLOUT_TONE_ALIASES.get(word);
  const title = (tone === undefined ? args : rest.join(" ")) || undefined;
  return { kind: "callout", tone: tone ?? "info", title, body };
}

// `status | item | note` rows. Models sometimes add a fourth location cell as in `ui-findings`; it is accepted only
// when it is path-shaped, so a genuinely extra column of prose still invalidates the component.
// `状态 | 事项 | 说明` 行。模型有时仿照 `ui-findings` 追加第四格位置；仅当其呈路径形态时接受，
// 因此真正多出的说明列仍使组件无效。
function checklist(body: string): AnswerComponent | null {
  const cells = (line: string) => {
    const plain = rowCells(line, 2, 3);
    if (plain !== null) return plain;
    const located = rowCells(line, 4, 4);
    return located !== null && answerLocationPath(located[3]) !== null ? located : null;
  };
  return mapLines(body, cells, ([status, item, note, location]) => (
    STATUSES.has(status) ? { status: status as AnswerStatus, item, note, location } : null
  ), (items) => ({ kind: "checklist", items }));
}

// `path:start-end | highlights | note` rows; at most ANSWER_CODE_MAX_EXCERPTS, each range at most
// ANSWER_CODE_MAX_LINES lines. Highlights are comma-separated lines or ranges inside the excerpt, or `-`.
// `路径:起-迄 | 高亮 | 说明` 行；至多 ANSWER_CODE_MAX_EXCERPTS 行，每个范围至多 ANSWER_CODE_MAX_LINES 行。
// 高亮为摘录内以逗号分隔的行号或范围，或 `-`。
function code(body: string): AnswerComponent | null {
  const lines = dataLines(body);
  if (lines.length === 0 || lines.length > ANSWER_CODE_MAX_EXCERPTS) return null;
  const excerpts: AnswerCodeExcerpt[] = [];
  for (const line of lines) {
    const cells = rowCells(line, 1, 3);
    const location = cells === null ? null : answerCodeLocation(cells[0]);
    if (cells === null || location === null) return null;
    const highlighted = cells[1] === undefined || cells[1] === "-" ? [] : lineSet(cells[1], location.startLine, location.endLine);
    if (highlighted === null) return null;
    excerpts.push({ ...location, highlighted, note: cells[2] });
  }
  return { kind: "code", excerpts };
}

/**
 * Parse `path:start-end` or `path:line`, splitting at the last colon so drive letters stay in the path.
 * Mirrors `parse_answer_code_location` in vulcan-session-core, which decides what the host captures.
 * 解析 `path:start-end` 或 `path:line`，在最后一个冒号处拆分，使盘符留在路径中。
 * 与 vulcan-session-core 的 `parse_answer_code_location` 一致，后者决定宿主抓取的内容。
 */
export function answerCodeLocation(cell: string): Omit<AnswerCodeExcerpt, "highlighted" | "note"> | null {
  const location = cell.trim();
  const split = location.lastIndexOf(":");
  if (split < 0) return null;
  const path = location.slice(0, split);
  const match = /^(\d+)(?:-(\d+))?$/u.exec(location.slice(split + 1));
  if (!match || !path || path.trim() !== path) return null;
  const startLine = Number(match[1]);
  const endLine = Number(match[2] ?? match[1]);
  // Line numbers are unsigned 32-bit on the host.
  // 宿主端行号为无符号 32 位整数。
  if (startLine < 1 || endLine < startLine || endLine > 0xffff_ffff || endLine - startLine + 1 > ANSWER_CODE_MAX_LINES) return null;
  return { location, path, startLine, endLine };
}

// Sorted distinct line numbers of `42,47-49`, all within `first..=last`; null when malformed.
// `42,47-49` 中排序去重的行号，全部位于 `first..=last` 之内；格式错误时为 null。
function lineSet(text: string, first: number, last: number): number[] | null {
  const lines = new Set<number>();
  for (const part of text.split(",")) {
    const match = /^(\d+)(?:-(\d+))?$/u.exec(part.trim());
    if (!match) return null;
    const from = Number(match[1]);
    const to = Number(match[2] ?? match[1]);
    if (from < first || to > last || to < from) return null;
    for (let line = from; line <= to; line += 1) lines.add(line);
  }
  return [...lines].sort((a, b) => a - b);
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

// Mermaid node outlines, longer openings first, with the closest `ui-flow` outline.
// Mermaid 节点轮廓（较长的开口在前）及最接近的 `ui-flow` 轮廓。
const MERMAID_OUTLINES: [string, string, AnswerFlowShape][] = [
  ["([", "])", "terminal"], ["[[", "]]", "box"], ["((", "))", "terminal"], ["{{", "}}", "box"], ["[/", "/]", "box"],
  ["[\\", "\\]", "box"], ["[(", ")]", "store"], ["[", "]", "box"], ["(", ")", "terminal"], ["{", "}", "decision"],
];

// A Mermaid-style node `ID[text]`: an identifier followed directly by an outline with nonempty text. `paren` marks
// the plain `(` outline, which also reads as a call such as `f(x)`.
// Mermaid 风格节点 `ID[文本]`：标识后紧跟带非空文本的轮廓。`paren` 标出普通 `(` 轮廓，它也可能是 `f(x)` 这样的调用。
function mermaidNode(spec: string): { id: string; label: string; shape: AnswerFlowShape; paren: boolean } | null {
  const match = /^([A-Za-z_][\w-]*)([[({].*)$/u.exec(spec);
  if (!match) return null;
  const outline = match[2];
  for (const [open, close, shape] of MERMAID_OUTLINES) {
    if (outline.length >= open.length + close.length && outline.startsWith(open) && outline.endsWith(close)) {
      const label = outline.slice(open.length, outline.length - close.length).trim();
      return label ? { id: match[1], label, shape, paren: open === "(" } : null;
    }
  }
  return null;
}

// `ui-flow [LR]`: one chain per line; nodes keep their first outline unless a later one is explicit. Models used to
// Mermaid write `B{OK?}` and then `B`: such an ID names the node `OK?` throughout the fence. A plain `(` outline counts
// only when its ID also stands alone, so `f(x)` stays text.
// `ui-flow [LR]`：每行一条链；节点保留首个轮廓，除非之后显式指定。习惯 Mermaid 的模型会写 `B{OK?}` 再写 `B`：
// 这样的标识在整个围栏中都指节点 `OK?`。普通 `(` 轮廓只有在其标识也单独出现时才算，因此 `f(x)` 保持为文字。
function flow(horizontal: boolean, body: string): AnswerComponent | null {
  const lines = dataLines(body);
  if (lines.length === 0 || lines.length > ANSWER_COMPONENT_MAX_ROWS) return null;
  const parsedLines = lines.map((line) => {
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
    if (parts.length === 0) return { mark, groups: [[text.trim()]], dashed, lone: true };
    const { head, tail } = labelled(rest);
    parts.push(head);
    return { mark, groups: parts.map((part) => part.split("&").map((spec) => spec.trim())), dashed, label: tail, lone: false };
  });
  const specs = parsedLines.flatMap((line) => line.groups.flat());
  const standalone = new Set(specs);
  const aliases = new Map<string, string>();
  for (const spec of specs) {
    const mermaid = mermaidNode(spec);
    if (mermaid && (!mermaid.paren || standalone.has(mermaid.id))) aliases.set(mermaid.id, mermaid.label);
  }
  const nodes: AnswerFlowNode[] = [];
  const ids = new Map<string, number>();
  const chains: AnswerFlowChain[] = [];
  const changed = new Set<number>();
  const node = (spec: string): number | null => {
    const mermaid = mermaidNode(spec);
    const alias = aliases.get(spec);
    const parsed = mermaid && aliases.has(mermaid.id) ? mermaid
      : alias !== undefined ? { label: alias, shape: "box" as const }
        : flowNode(spec);
    if (parsed === null) return null;
    let id = ids.get(parsed.label);
    if (id === undefined) {
      id = nodes.push({ label: parsed.label, shape: "box" }) - 1;
      ids.set(parsed.label, id);
    }
    if (parsed.shape !== "box") nodes[id].shape = parsed.shape;
    return id;
  };
  for (const { mark, groups: specGroups, dashed, label, lone } of parsedLines) {
    if (lone) {
      const id = node(specGroups[0][0]);
      if (id === null) return null;
      if (mark === "chg") changed.add(id);
      chains.push({ groups: [[id]], dashed: [], mark });
      continue;
    }
    const groups: number[][] = [];
    for (const part of specGroups) {
      const group = part.map(node);
      if (group.some((id) => id === null)) return null;
      groups.push(group as number[]);
    }
    chains.push({ groups, dashed, label: label || undefined, mark });
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

const ER_KEYS = new Set<string>(["PK", "FK", "UK"]);

// Split a trailing `| note` from a line into a trimmed head and an optional nonempty note.
// 从一行拆出末尾的 `| 说明`，得到去空白的前段与可选的非空说明。
function noted(text: string): { head: string; note?: string } {
  const split = text.indexOf("|");
  if (split < 0) return { head: text.trim() };
  return { head: text.slice(0, split).trim(), note: text.slice(split + 1).trim() || undefined };
}

// `ui-er [LR|TB]`: unindented lines name tables, indented lines are `name [type] [PK|FK|UK] [-> Table]`
// fields; every reference must name a table of the same diagram.
// `ui-er [LR|TB]`：无缩进行为表名，缩进行为 `名称 [类型] [PK|FK|UK] [-> 表]` 字段；每个引用都必须指向同图中的表。
function er(horizontal: boolean, body: string): AnswerComponent | null {
  const lines = body.split(/\r?\n/u).filter((line) => line.trim());
  if (lines.length === 0 || lines.length > ANSWER_COMPONENT_MAX_ROWS) return null;
  const tables: AnswerErTable[] = [];
  const references: { field: AnswerErField; target: string }[] = [];
  for (const line of lines) {
    const { mark, text } = change(line.trim());
    const { head, note } = noted(text);
    if (!head) return null;
    if (!/^\s/u.test(line)) {
      tables.push({ name: head, note, mark, fields: [] });
      continue;
    }
    if (tables.length === 0) return null;
    const arrow = head.indexOf("->");
    const target = arrow < 0 ? undefined : head.slice(arrow + 2).trim();
    if (target === "") return null;
    const [name, ...rest] = (arrow < 0 ? head : head.slice(0, arrow)).trim().split(/\s+/u).filter(Boolean);
    if (!name) return null;
    const keys = rest.filter((token) => ER_KEYS.has(token)) as AnswerErKey[];
    if (target !== undefined && !keys.includes("FK")) keys.push("FK");
    const type = rest.filter((token) => !ER_KEYS.has(token)).join(" ") || undefined;
    const field: AnswerErField = { name, type, keys, note, mark };
    tables[tables.length - 1].fields.push(field);
    if (target !== undefined) references.push({ field, target });
  }
  const names = tables.map((table) => table.name);
  if (new Set(names).size !== names.length) return null;
  for (const reference of references) {
    const index = names.indexOf(reference.target);
    if (index < 0) return null;
    reference.field.target = index;
  }
  return { kind: "er", horizontal, tables };
}

// `ui-math [title]`: formula lines, then after a blank line optional `symbol | meaning` legend rows. Each
// formula line is one displayed equation unless the formula uses a `\begin` environment.
// Models also separate groups of equations with blank lines, so only the group after the last blank line can be
// the legend: it is one when every row is `symbol | meaning`, further formulas when none is, and a mix is invalid.
// Every earlier group is formulas.
// `ui-math [标题]`：公式行，空行之后为可选的 `符号 | 含义` 图例行。除非公式使用 `\begin` 环境，每行公式各自独立显示。
// 模型也会用空行分隔多组公式，因此只有最后一个空行之后的一组可能是图例：全部为 `符号 | 含义` 行时为图例，
// 都不是时按公式处理，两者混杂则无效。之前的各组都是公式。
function math(args: string, body: string): AnswerComponent | null {
  const lines = body.split(/\r?\n/u);
  while (lines.length > 0 && !lines[0].trim()) lines.shift();
  while (lines.length > 0 && !lines[lines.length - 1].trim()) lines.pop();
  let blank = lines.length - 1;
  while (blank >= 0 && lines[blank].trim()) blank -= 1;
  const after = blank < 0 ? [] : dataLines(lines.slice(blank + 1).join("\n"));
  const legendRows = after.map((line) => rowCells(line, 2, 2));
  const isLegend = legendRows.every((cells) => cells !== null);
  if (!isLegend && legendRows.some((cells) => cells !== null)) return null;
  const formula = (blank < 0 ? lines : lines.slice(0, blank)).map((line) => line.trim()).filter(Boolean)
    .concat(isLegend ? [] : after.map((line) => line.trim()));
  const legend = isLegend ? legendRows as string[][] : [];
  if (formula.length === 0 || formula.length + legend.length > ANSWER_COMPONENT_MAX_ROWS) return null;
  const symbols: AnswerMathSymbol[] = legend.map(([symbol, meaning]) => ({ symbol, meaning }));
  const equations = formula.some((line) => line.includes("\\begin")) ? [formula.join("\n")] : formula;
  return { kind: "math", title: args || undefined, equations, symbols };
}

// `ui-decision [multi]`: the question line, then option lines `* label | note` (the recommended option) or
// `- label | note`. Labels are unique; a single choice recommends at most one option.
// `ui-decision [multi]`：问题行，然后是选项行 `* 标签 | 说明`（建议选项）或 `- 标签 | 说明`。标签不可重复；单选最多建议一个选项。
function decision(multiple: boolean, body: string): AnswerComponent | null {
  const [question = "", ...lines] = body.split(/\r?\n/u).map((line) => line.trim()).filter(Boolean);
  if (/^[*-] /u.test(question) || lines.length < 2 || lines.length > ANSWER_COMPONENT_MAX_ROWS) return null;
  const options: AnswerDecisionOption[] = [];
  for (const line of lines) {
    const marker = line.slice(0, 2);
    const cells = marker === "* " || marker === "- " ? rowCells(line.slice(2), 1, 2) : null;
    if (cells === null || options.some((option) => option.label === cells[0])) return null;
    options.push({ label: cells[0], note: cells[1], recommended: marker === "* " });
  }
  if (!multiple && options.filter((option) => option.recommended).length > 1) return null;
  return { kind: "decision", multiple, question, options };
}
