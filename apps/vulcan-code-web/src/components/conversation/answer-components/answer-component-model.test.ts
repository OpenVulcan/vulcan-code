import { describe, expect, it } from "vitest";
import { ANSWER_CHART_MAX_SERIES, ANSWER_CODE_MAX_EXCERPTS, ANSWER_CODE_MAX_LINES, ANSWER_COMPONENT_MAX_ROWS } from "@/generated/protocol-limits";
import { parseAnswerComponent, type AnswerComponent } from "./answer-component-model";

// parse wraps the parser for fences written as `language meta`.
// parse 为以 `语言 参数` 书写的围栏包装解析器。
function parse(info: string, body: string): AnswerComponent | null {
  const [language, ...meta] = info.split(" ");
  return parseAnswerComponent(language, meta.join(" "), body);
}

describe("parseAnswerComponent", () => {
  it("rejects unknown names and blank bodies", () => {
    expect(parse("ui-unknown", "a | b")).toBeNull();
    expect(parse("ui-kv", "  \n ")).toBeNull();
    expect(parse("ui-callout", " \n")).toBeNull();
    expect(parse("ui-details", "body")).toBeNull();
  });

  it("reads text after a row component name as its heading", () => {
    expect(parse("ui-kv Round trip", "a | b")).toEqual({ kind: "kv", heading: "Round trip", rows: [{ label: "a", value: "b" }] });
    expect(parse("ui-kv", "a | b")).not.toHaveProperty("heading");
    expect(parse("ui-checklist Checks", "ok | item")?.heading).toBe("Checks");
    expect(parse("ui-kv Round trip", "a | b | c")).toBeNull();
  });

  it("rejects rows with empty cells, wrong cell counts or unknown status words", () => {
    expect(parse("ui-kv", "a | ")).toBeNull();
    expect(parse("ui-kv", "a | b | c")).toBeNull();
    expect(parse("ui-checklist", "done | item")).toBeNull();
    expect(parse("ui-steps", "ok | step")).toBeNull();
    expect(parse("ui-findings", "critical | issue")).toBeNull();
  });

  it("enforces the shared row limit", () => {
    const rows = (count: number) => Array.from({ length: count }, (_, index) => `k${index} | v`).join("\n");
    expect(parse("ui-kv", rows(ANSWER_COMPONENT_MAX_ROWS))).not.toBeNull();
    expect(parse("ui-kv", rows(ANSWER_COMPONENT_MAX_ROWS + 1))).toBeNull();
  });

  it("skips a Markdown separator row and optional outer pipes", () => {
    expect(parse("ui-kv", "| a | b |\n|---|---|\n| c | d |")).toEqual({ kind: "kv", rows: [{ label: "a", value: "b" }, { label: "c", value: "d" }] });
  });

  it("validates metric meters and splits a trailing tone word", () => {
    const metrics = parse("ui-metrics", "Coverage | 80 | 100 | % | ok\nOpen | 3 | warn");
    expect(metrics).toMatchObject({ kind: "metrics", rows: [{ numeric: 80, maximum: 100, unit: "%", tone: "ok" }, { value: "3", tone: "warn" }] });
    expect(parse("ui-metrics", "Coverage | high | 100")).toBeNull();
    expect(parse("ui-metrics", "Coverage | 1 | 0")).toBeNull();
    expect(parse("ui-metrics", "Coverage | -1 | 100")).toBeNull();
  });

  it("reads a lone non-numeric third metric cell as the unit, without a meter", () => {
    expect(parse("ui-metrics", "Files | 9\nScan time | 10.9 | s\nOutput | 59 | MB | warn")).toMatchObject({
      kind: "metrics", rows: [{ value: "9" }, { value: "10.9", unit: "s" }, { value: "59", unit: "MB", tone: "warn" }],
    });
    expect(parse("ui-metrics", "Scan time | 10.9 | s")).not.toHaveProperty("rows.0.maximum");
    // A non-numeric maximum followed by a unit is still a malformed meter.
    expect(parse("ui-metrics", "Coverage | 80 | high | %")).toBeNull();
  });

  it("treats empty maximum and unit cells as absent, keeping other empty cells malformed", () => {
    // Same column shape as the sample that failed to render, with marker labels.
    const body = "raw-rg | 506 s | | | no\nfixed-rg | 2.58 s\ncopy-size | 0.96 | | MB | ok";
    expect(parse("ui-metrics", body)).toMatchObject({
      kind: "metrics", rows: [
        { label: "raw-rg", value: "506 s", tone: "no" },
        { value: "2.58 s" },
        { value: "0.96", unit: "MB", tone: "ok" },
      ],
    });
    expect(parse("ui-metrics", "Coverage | 80 | | %")).not.toHaveProperty("rows.0.maximum");
    expect(parse("ui-metrics", " | 80")).toBeNull();
    expect(parse("ui-metrics", "Coverage | ")).toBeNull();
    // Outer pipes are optional, so an empty fifth cell needs a closing pipe after it.
    expect(parse("ui-metrics", "Coverage | 80 | 100 | % |  |")).toBeNull();
  });

  it("marks a value above its maximum as an overrun whose tone defaults to no", () => {
    expect(parse("ui-metrics", "Images | 620 | 500 | KB\nScripts | 200 | 170 | KB | warn\nFonts | 40 | 50")).toMatchObject({
      kind: "metrics", rows: [{ over: true, tone: "no" }, { over: true, tone: "warn" }, { over: false }],
    });
    expect(parse("ui-metrics", "Fonts | 40 | 50")).toHaveProperty("rows.0.tone", undefined);
  });

  it("reads entity tables, field keys and references, and rejects unknown targets", () => {
    const er = parse("ui-er", "User | accounts\n  id PK\n  email string UK\n+ Order\n  user_id -> User\n  ~ total decimal(10, 2) | gross");
    expect(er).toMatchObject({
      kind: "er", horizontal: true, tables: [
        { name: "User", note: "accounts", fields: [{ name: "id", keys: ["PK"] }, { name: "email", type: "string", keys: ["UK"] }] },
        { name: "Order", mark: "add", fields: [{ name: "user_id", keys: ["FK"], target: 0 }, { name: "total", type: "decimal(10, 2)", note: "gross", mark: "chg" }] },
      ],
    });
    expect(parse("ui-er TB", "A\n  id PK")).toMatchObject({ horizontal: false });
    expect(parse("ui-er", "A\n  b_id -> B")).toBeNull();
    expect(parse("ui-er", "  id PK\nA")).toBeNull();
    expect(parse("ui-er", "A\nA")).toBeNull();
    expect(parse("ui-er RL", "A")).toBeNull();
  });

  it("splits formula lines from a legend after the first blank line", () => {
    expect(parse("ui-math Little's law", "L = \\lambda W\n\\rho = \\lambda / \\mu\n\nL | Requests in system\n\\lambda | Arrival rate")).toEqual({
      kind: "math", title: "Little's law", equations: ["L = \\lambda W", "\\rho = \\lambda / \\mu"],
      symbols: [{ symbol: "L", meaning: "Requests in system" }, { symbol: "\\lambda", meaning: "Arrival rate" }],
    });
    expect(parse("ui-math", "\\begin{aligned}\na &= b \\\\\n&= c\n\\end{aligned}")).toMatchObject({ equations: ["\\begin{aligned}\na &= b \\\\\n&= c\n\\end{aligned}"] });
    expect(parse("ui-math", "|x| = 1")).toMatchObject({ equations: ["|x| = 1"], symbols: [] });
    expect(parse("ui-math", "x = 1\n\nx | ok\ny")).toBeNull();
  });

  it("reads blank-separated formula groups before a trailing legend", () => {
    expect(parse("ui-math", "q = n / 86400\ng = q / 24.5\n\nn = 50 \\Rightarrow 0.24\nn = 500 \\Rightarrow 2.4\n\nn | Calls per user per day\n24.5 | QPS of one GPU\n")).toEqual({
      kind: "math", title: undefined,
      equations: ["q = n / 86400", "g = q / 24.5", "n = 50 \\Rightarrow 0.24", "n = 500 \\Rightarrow 2.4"],
      symbols: [{ symbol: "n", meaning: "Calls per user per day" }, { symbol: "24.5", meaning: "QPS of one GPU" }],
    });
    expect(parse("ui-math", "a = 1\n\nb = 2\n\nc = 3")).toMatchObject({ equations: ["a = 1", "b = 2", "c = 3"], symbols: [] });
    expect(parse("ui-math", "a = 1\n\nb = 2\n\nb | ok\nc = 3")).toBeNull();
  });

  it("reads lines after a blank line as further formulas when none is a legend row", () => {
    expect(parse("ui-math Rate", "r = a / b\n\nso: \\quad r = a / c\ns = 1")).toEqual({
      kind: "math", title: "Rate", equations: ["r = a / b", "so: \\quad r = a / c", "s = 1"], symbols: [],
    });
  });

  it("sums file deltas, including one-sided counts", () => {
    expect(parse("ui-files", "a.rs | +9 -2 | note\nb.rs | +1 −0\nc.md | only a note")).toMatchObject({
      kind: "files", added: 10, removed: 2, files: [{ added: 9, note: "note" }, { removed: 0 }, { note: "only a note" }],
    });
    expect(parse("ui-files", "a.rs | +145 | entry\nb.rs | -3\nc.rs | +2 -1")).toMatchObject({
      kind: "files", added: 147, removed: 4, files: [{ added: 145, note: "entry" }, { removed: 3 }, { added: 2, removed: 1 }],
    });
    expect(parse("ui-files", "a.rs | +145")).not.toHaveProperty("files.0.removed");
  });

  it("reads a change label written in place of the file counts", () => {
    expect(parse("ui-files", "a.ts | new | maps cwd to a project\nb.yaml | changed | lockfile")).toMatchObject({
      kind: "files", added: 0, removed: 0, files: [{ path: "a.ts", status: "new", note: "maps cwd to a project" }, { status: "changed" }],
    });
  });

  it("reads an empty file counts cell as a row with only a note", () => {
    const files = parse("ui-files", "a.tsx | | folds tool cards\nb.css | +3 -1 | keyframes");
    expect(files).toMatchObject({ kind: "files", added: 3, removed: 1, files: [{ path: "a.tsx", note: "folds tool cards" }, { added: 3 }] });
    expect(files).not.toHaveProperty("files.0.status");
    expect(parse("ui-files", "| | note")).toBeNull();
    expect(parse("ui-files", "a.tsx | | note | extra")).toBeNull();
  });

  it("reads a callout without a tone as info and maps row tone words", () => {
    expect(parse("ui-callout", "body")).toEqual({ kind: "callout", tone: "info", title: undefined, body: "body" });
    expect(parse("ui-callout Mind the gap", "body")).toMatchObject({ tone: "info", title: "Mind the gap" });
    expect(parse("ui-callout warn Back up", "body")).toMatchObject({ tone: "warning", title: "Back up" });
    expect(parse("ui-callout no", "body")).toMatchObject({ tone: "danger", title: undefined });
    expect(parse("ui-callout constructor", "body")).toMatchObject({ tone: "info", title: "constructor" });
  });

  it("reads a fourth findings cell as a note only after the documented split fails", () => {
    expect(parse("ui-findings", "high | require() of ESM | src/zip.js:10 | baseline is red too\nmedium | 653 rows unchecked | ledger | add evidence")).toMatchObject({
      kind: "findings",
      rows: [
        { severity: "high", text: "require() of ESM", location: "src/zip.js:10", note: "baseline is red too" },
        { severity: "medium", location: "ledger", note: "add evidence" },
      ],
    });
    expect(parse("ui-findings", "info | adversarial (fetch|iex) cases | src/a.rs")).toMatchObject({
      rows: [{ text: "adversarial (fetch|iex) cases", location: "src/a.rs", note: undefined }],
    });
    expect(parse("ui-findings", "high | a | b | c | d")).toBeNull();
  });

  it("accepts a fourth checklist cell only when it is a path-shaped location", () => {
    expect(parse("ui-checklist", "no | Sums reasoning twice | double count | crates/a/src/turn_stats.rs:150\nok | Builds")).toMatchObject({
      kind: "checklist", items: [{ status: "no", note: "double count", location: "crates/a/src/turn_stats.rs:150" }, { item: "Builds" }],
    });
    expect(parse("ui-checklist", "warn | item | note | needs a second look")).toBeNull();
    expect(parse("ui-checklist", "warn | item | note | src/a.rs | extra")).toBeNull();
  });

  it("parses compare matrices with tone cells and requires matching widths", () => {
    expect(parse("ui-compare", "Criterion | A | B\nCost | ok Cheap | no")).toMatchObject({
      kind: "compare", rows: [{ criterion: "Cost", cells: [{ tone: "ok", text: "Cheap" }, { tone: "no", text: "" }] }],
    });
    expect(parse("ui-compare", "Criterion | A\nCost | ok")).toBeNull();
    expect(parse("ui-compare", "Criterion | A | B\nCost | ok")).toBeNull();
  });

  it("keeps escaped, code and word-joined pipes inside a cell", () => {
    expect(parse("ui-findings", "info | runs fetch\\|iex | src/a.rs")).toMatchObject({
      kind: "findings", rows: [{ text: "runs fetch|iex", location: "src/a.rs" }],
    });
    expect(parse("ui-findings", "info | adversarial (fetch|iex) cases | src/a.rs")).toMatchObject({
      kind: "findings", rows: [{ text: "adversarial (fetch|iex) cases", location: "src/a.rs" }],
    });
    expect(parse("ui-compare", "Criterion | A | B\nExample | `a | b` | Invoke-WebRequest|iex")).toMatchObject({
      kind: "compare", rows: [{ cells: [{ text: "`a | b`" }, { text: "Invoke-WebRequest|iex" }] }],
    });
    // Spaced pipes still separate cells, so a genuinely extra column is rejected.
    // 带空格的竖线仍然分隔单元格，真正多出的列仍被拒绝。
    expect(parse("ui-compare", "Criterion | A | B\nCost | ok | no | extra")).toBeNull();
  });

  it("validates chart kinds, series counts and value signs", () => {
    expect(parse("ui-chart bar Weekly", "Week | New | Fixed\nW1 | 3 | 2%")).toMatchObject({
      kind: "chart", chart: { kind: "bar", title: "Weekly", categoryLabel: "Week", series: ["New", "Fixed"], rows: [{ values: [3, 2] }] },
    });
    expect(parse("ui-chart radar", "A | B\nx | 1")).toBeNull();
    expect(parse("ui-chart pie", "A | B | C\nx | 1 | 2")).toBeNull();
    expect(parse("ui-chart pie", "A | B\nx | -1\ny | 3")).toBeNull();
    expect(parse("ui-chart donut", "A | B\nx | 0")).toBeNull();
    expect(parse("ui-chart bar", "A | B\nx | many")).toBeNull();
    const header = ["Category", ...Array.from({ length: ANSWER_CHART_MAX_SERIES + 1 }, (_, index) => `S${index}`)];
    expect(parse("ui-chart line", `${header.join(" | ")}\n${["x", ...header.slice(1).map(() => "1")].join(" | ")}`)).toBeNull();
  });

  it("requires at least two nonempty sections for tabs and columns", () => {
    expect(parse("ui-tabs", "### A\none\n### B\ntwo")).toMatchObject({ kind: "tabs", sections: [{ title: "A", body: "one" }, { title: "B", body: "two" }] });
    expect(parse("ui-columns", "### A\none")).toBeNull();
    expect(parse("ui-tabs", "intro\n### A\none\n### B\ntwo")).toBeNull();
    expect(parse("ui-tabs", "### A\n### B\ntwo")).toBeNull();
  });

  it("nests tree nodes by indentation and reads plan markers", () => {
    expect(parse("ui-tree", "src\n  + new.ts | added\n  - old.ts\n  \\- dash.ts")).toMatchObject({
      kind: "tree", nodes: [{ label: "src", children: [{ label: "new.ts", note: "added", mark: "add" }, { label: "old.ts", mark: "del" }, { label: "- dash.ts" }] }],
    });
  });

  it("treats the same flow label as one node and keeps lone declarations out of the drawing", () => {
    const flow = parse("ui-flow LR", "(Start) -> {Valid?} -> Save: yes\nValid? --> Reject & Log: no\n~ Save\n+ Save -> Notify");
    expect(flow?.kind).toBe("flow");
    if (flow?.kind !== "flow") return;
    expect(flow.horizontal).toBe(true);
    expect(flow.nodes.map((node) => [node.label, node.shape])).toEqual([
      ["Start", "terminal"], ["Valid?", "decision"], ["Save", "box"], ["Reject", "box"], ["Log", "box"], ["Notify", "box"],
    ]);
    expect(flow.nodes.find((node) => node.label === "Save")?.mark).toBe("chg");
    expect(flow.nodes.find((node) => node.label === "Notify")?.mark).toBe("add");
    expect(flow.chains).toHaveLength(3);
    expect(flow.chains[1]).toMatchObject({ groups: [[1], [3, 4]], dashed: [true], label: "no" });
    expect(parse("ui-flow TB", "A -> B")).toBeNull();
    expect(parse("ui-flow", "A -> ")).toBeNull();
  });

  it("reads a Mermaid node ID as the node it names, keeping call-like labels as text", () => {
    const flow = parse("ui-flow", "A([Start]) -> B{OK?}\nB -> C[(Orders)]: yes\nB --> D[Retry]\nD -> B");
    if (flow?.kind !== "flow") throw new Error("expected a flow");
    expect(flow.nodes.map((node) => [node.label, node.shape])).toEqual([
      ["Start", "terminal"], ["OK?", "decision"], ["Orders", "store"], ["Retry", "box"],
    ]);
    expect(flow.chains.map((chain) => chain.groups)).toEqual([[[0], [1]], [[1], [2]], [[1], [3]], [[3], [1]]]);
    const calls = parse("ui-flow", "load(config) -> init() -> run");
    if (calls?.kind !== "flow") throw new Error("expected a flow");
    expect(calls.nodes.map((node) => node.label)).toEqual(["load(config)", "init()", "run"]);
    const referenced = parse("ui-flow", "S(Start) -> Work\nWork -> S");
    if (referenced?.kind !== "flow") throw new Error("expected a flow");
    expect(referenced.nodes.map((node) => [node.label, node.shape])).toEqual([["Start", "terminal"], ["Work", "box"]]);
  });

  it("orders sequence actors by first use and records direction, notes and phases", () => {
    const sequence = parse("ui-sequence num", "== Login ==\nUser -> API: request\nAPI --> User: token\nnote User, API: cached\n- API -> API: retry");
    expect(sequence).toMatchObject({
      kind: "sequence", numbered: true, actors: ["User", "API"], steps: [
        { kind: "phase", text: "Login" },
        { kind: "message", from: 0, to: 1, dashed: false, text: "request" },
        { kind: "message", from: 1, to: 0, dashed: true, text: "token" },
        { kind: "note", first: 0, last: 1, text: "cached" },
        { kind: "message", from: 1, to: 1, mark: "del", text: "retry" },
      ],
    });
    expect(parse("ui-sequence", "User talks to API")).toBeNull();
    expect(parse("ui-sequence", "note User")).toBeNull();
  });

  it("accepts the row-component pipe in place of the colon before message, note and flow labels", () => {
    expect(parse("ui-sequence", "User -> API | request\nnote User, API | cached")).toMatchObject({
      actors: ["User", "API"], steps: [
        { kind: "message", from: 0, to: 1, text: "request" },
        { kind: "note", first: 0, last: 1, text: "cached" },
      ],
    });
    expect(parse("ui-flow", "Build -> Test | on push")).toMatchObject({ chains: [{ groups: [[0], [1]], label: "on push" }] });
  });

  it("reads a decision question, its options and the recommendation", () => {
    expect(parse("ui-decision", "Cache lifetime?\n* 10 minutes | Matches deploys\n- 1 hour")).toEqual({
      kind: "decision", multiple: false, question: "Cache lifetime?", options: [
        { label: "10 minutes", note: "Matches deploys", recommended: true },
        { label: "1 hour", note: undefined, recommended: false },
      ],
    });
    expect(parse("ui-decision multi", "Also?\n* A\n* B")).toMatchObject({ multiple: true });
  });

  it("rejects decisions without two marked options, with repeated labels or with two recommendations", () => {
    expect(parse("ui-decision", "Q?\n* Only one")).toBeNull();
    expect(parse("ui-decision", "Q?\nA\n- B")).toBeNull();
    expect(parse("ui-decision", "Q?\n- A\n- A")).toBeNull();
    expect(parse("ui-decision", "Q?\n* A\n* B")).toBeNull();
    expect(parse("ui-decision", "- Q?\n- A\n- B")).toBeNull();
    expect(parse("ui-decision single", "Q?\n- A\n- B")).toBeNull();
  });

  it("parses code excerpts with ranges, single lines, drive letters, highlights and notes", () => {
    expect(parse("ui-code", "src/a.rs:40-45 | 42, 44-45 | Lock first\nC:\\repo\\b.ts:7\nsrc/c.rs:1-3 | - | Entry")).toEqual({
      kind: "code",
      excerpts: [
        { location: "src/a.rs:40-45", path: "src/a.rs", startLine: 40, endLine: 45, highlighted: [42, 44, 45], note: "Lock first" },
        { location: "C:\\repo\\b.ts:7", path: "C:\\repo\\b.ts", startLine: 7, endLine: 7, highlighted: [], note: undefined },
        { location: "src/c.rs:1-3", path: "src/c.rs", startLine: 1, endLine: 3, highlighted: [], note: "Entry" },
      ],
    });
  });

  it("rejects code excerpts without a valid range or with highlights outside it", () => {
    for (const row of ["src/a.rs", "src/a.rs:0-2", "src/a.rs:5-3", "src/a.rs:1-", "src/a.rs:1-3 | 4", "src/a.rs:1-3 | x"]) {
      expect(parse("ui-code", row), row).toBeNull();
    }
    expect(parse("ui-code", `a.rs:1-${ANSWER_CODE_MAX_LINES}`)).not.toBeNull();
    expect(parse("ui-code", `a.rs:1-${ANSWER_CODE_MAX_LINES + 1}`)).toBeNull();
    const rows = (count: number) => Array.from({ length: count }, (_, index) => `a.rs:${index + 1}`).join("\n");
    expect(parse("ui-code", rows(ANSWER_CODE_MAX_EXCERPTS))).not.toBeNull();
    expect(parse("ui-code", rows(ANSWER_CODE_MAX_EXCERPTS + 1))).toBeNull();
  });

  it("reads text after the ui-code name as its heading", () => {
    expect(parse("ui-code Key lines", "a.rs:1-2 | 2 | note")).toMatchObject({ kind: "code", heading: "Key lines", excerpts: [{ location: "a.rs:1-2" }] });
  });
});
