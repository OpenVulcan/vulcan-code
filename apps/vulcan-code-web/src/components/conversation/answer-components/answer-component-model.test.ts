import { describe, expect, it } from "vitest";
import { ANSWER_CHART_MAX_SERIES, ANSWER_COMPONENT_MAX_ROWS } from "@/generated/protocol-limits";
import { parseAnswerComponent, type AnswerComponent } from "./answer-component-model";

// parse wraps the parser for fences written as `language meta`.
// parse 为以 `语言 参数` 书写的围栏包装解析器。
function parse(info: string, body: string): AnswerComponent | null {
  const [language, ...meta] = info.split(" ");
  return parseAnswerComponent(language, meta.join(" "), body);
}

describe("parseAnswerComponent", () => {
  it("rejects unknown names, blank bodies and arguments on row components", () => {
    expect(parse("ui-unknown", "a | b")).toBeNull();
    expect(parse("ui-kv", "  \n ")).toBeNull();
    expect(parse("ui-kv extra", "a | b")).toBeNull();
    expect(parse("ui-callout", "body")).toBeNull();
    expect(parse("ui-details", "body")).toBeNull();
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
    expect(parse("ui-metrics", "Coverage | 120 | 100")).toBeNull();
    expect(parse("ui-metrics", "Coverage | high | 100")).toBeNull();
    expect(parse("ui-metrics", "Coverage | 1 | 0")).toBeNull();
  });

  it("sums file deltas and rejects a third cell without a delta", () => {
    expect(parse("ui-files", "a.rs | +9 -2 | note\nb.rs | +1 −0\nc.md | only a note")).toMatchObject({
      kind: "files", added: 10, removed: 2, files: [{ added: 9, note: "note" }, { removed: 0 }, { note: "only a note" }],
    });
    expect(parse("ui-files", "a.rs | changed | note")).toBeNull();
  });

  it("parses compare matrices with tone cells and requires matching widths", () => {
    expect(parse("ui-compare", "Criterion | A | B\nCost | ok Cheap | no")).toMatchObject({
      kind: "compare", rows: [{ criterion: "Cost", cells: [{ tone: "ok", text: "Cheap" }, { tone: "no", text: "" }] }],
    });
    expect(parse("ui-compare", "Criterion | A\nCost | ok")).toBeNull();
    expect(parse("ui-compare", "Criterion | A | B\nCost | ok")).toBeNull();
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
});
