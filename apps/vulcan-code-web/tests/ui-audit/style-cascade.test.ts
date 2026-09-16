import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { expect, it } from "vitest";

// load resolves the repository's existing Vite-owned CSS parser without adding a dependency.
// load 解析仓库既有的 Vite 所属 CSS 解析器，不引入依赖。
const load = createRequire(import.meta.url);
const postcss = createRequire(load.resolve("vite"))("postcss");

// desktopRules returns actual top-level rules in source order, excluding viewport-dependent branches.
// desktopRules 按源码顺序返回实际顶层规则，排除依赖视口的分支。
// Parameters: name selects a real stylesheet. Returns: desktop CSS for jsdom's limited cascade engine.
// 参数：name 选择真实样式表。返回值：供 jsdom 有限层叠引擎使用的桌面 CSS。
function desktopRules(name: string): string {
  // root is parsed from current source so moving a generic rule after its specialized override is observable.
  // root 从当前源码解析，使将通用规则移至专用覆盖之后的变化可被观察。
  const root = postcss.parse(readFileSync(resolve("src/styles", `${name}.css`), "utf8"));
  return root.nodes.filter((node: { type: string }) => node.type === "rule").map((node: { toString(): string }) => node.toString()).join("\n");
}

it("keeps the absolute thinking trigger inside a tool row without increasing its line height", () => {
  // style uses the actual card and gutter rules; no test-only dimensions hide overflow.
  // style 使用真实卡片与侧栏规则，不用测试专属尺寸掩盖溢出。
  const style = document.createElement("style");
  style.textContent = desktopRules("tool-presentations") + desktopRules("special-blocks");
  // row mirrors the last tool in a scrolling group, where the marker must not extend its scroll area.
  // row 对应滚动组末行工具，思考入口不得扩大该组滚动范围。
  const row = document.createElement("div");
  row.className = "assistant-flow tool-with-thinking";
  row.innerHTML = '<button class="thinking-preview-trigger compact"></button><div class="tool-presentation-card"><button class="tool-presentation-summary">command</button></div>';
  document.head.appendChild(style); document.body.appendChild(row);
  try {
    // summaryHeight derives the unchanged one-line summary box from source rules, excluding layout mocks.
    // summaryHeight 从源码规则计算未改变的单行摘要高度，不依赖布局模拟值。
    const rules = postcss.parse(desktopRules("tool-presentations"));
    // declaration avoids jsdom's incomplete inheritance handling for font: inherit.
    // declaration 避开 jsdom 对 font: inherit 不完整的继承处理。
    // Parameters select an exact source rule/member; returns its numeric value or fails if removed.
    // 参数选定精确源码规则与字段；返回数值，规则被移除时直接失败。
    const declaration = (selector: string, property: string): number => {
      const rule = rules.nodes.find((node: { selector: string }) => node.selector === selector);
      return Number.parseFloat(rule.nodes.find((node: { prop: string }) => node.prop === property).value);
    };
    const summaryHeight = declaration(".tool-presentation-card", "font-size") * declaration(".tool-presentation-summary", "line-height") + 2 * declaration(".tool-presentation-summary", "padding");
    const trigger = getComputedStyle(row.querySelector(".thinking-preview-trigger")!);
    expect(trigger.position).toBe("absolute");
    expect(Number.parseFloat(trigger.height)).toBeLessThanOrEqual(summaryHeight);
    expect(getComputedStyle(row).minHeight).not.toMatch(/^[1-9]/);
  } finally { row.remove(); style.remove(); }
});

it.each(["known", "generic", "v1", "skill"])("keeps %s payload shells unbounded while the inner source remains the scrolling viewport", (kind) => {
  // style follows the production Markdown-before-tool cascade.
  // style 遵循生产环境先 Markdown、后工具的层叠顺序。
  const style = document.createElement("style");
  style.textContent = desktopRules("markdown") + desktopRules("tool-presentations");
  // source represents the common code viewer used by readable results and raw input/output alike.
  // source 表示可读结果与原始输入输出共用的代码视图。
  const source = '<div class="markdown-message"><div class="markdown-code-block"><div class="markdown-code-header">markdown</div><div class="markdown-code-scroll">content</div></div></div>';
  const panel = `<div class="model-extension-payload-body"${kind === "skill" ? "" : ' role="tabpanel"'}>${kind === "known" ? `<div class="markdown-tool-block">${source}</div>` : source}</div>`;
  const tabs = kind === "skill" ? panel : `<div class="model-extension-tool-payloads">${panel}</div>`;
  const shell = document.createElement("div");
  shell.className = "tool-presentation-card";
  shell.innerHTML = `<div class="conversation-collapsible-motion open"><div class="conversation-collapsible-motion-inner">${kind === "v1" ? `<div class="v1-tool-card-detail">${tabs}</div>` : tabs}</div></div>`;
  document.head.appendChild(style); document.body.appendChild(shell);
  try {
    for (const element of shell.querySelectorAll(".model-extension-tool-payloads, .model-extension-payload-body, .v1-tool-card-detail")) {
      const computed = getComputedStyle(element);
      expect(["", "none"]).toContain(computed.maxHeight);
      expect(["", "visible"]).toContain(computed.overflow);
    }
    const block = getComputedStyle(shell.querySelector(".markdown-code-block")!);
    expect(block.maxHeight).toBe("420px");
    expect(block.display).toBe("flex");
    expect(getComputedStyle(shell.querySelector(".markdown-code-scroll")!).overflow).toBe("auto");
  } finally { shell.remove(); style.remove(); }
});

it("keeps multi-class agent toolbar overrides after generic styles when the lazy directory first loads", () => {
  // style follows production order: global rules first, deferred directory rules after navigation.
  // style 遵循生产顺序：先全局规则，导航后再载入延迟目录规则。
  const style = document.createElement("style");
  style.textContent = desktopRules("skill-cards") + desktopRules("skill-directory");
  // shell preserves the actual co-occurring classes rather than testing isolated selector names.
  // shell 保留实际同时出现的类，而非只测试孤立选择器名称。
  const shell = document.createElement("section");
  shell.className = "agent-management-surface";
  shell.innerHTML = '<div class="skill-page-body agent-skill-page-body"><section class="skill-toolbar agent-catalog-toolbar"></section><div class="skill-card-list agent-skill-card-list"></div></div><div class="agent-catalog-filter-row skill-list-filter-row"></div>';
  document.head.appendChild(style); document.body.appendChild(shell);
  try {
    // toolbar must retain the specialized grid and edge treatment after loading the lazy stylesheet.
    // toolbar 在载入懒加载样式后必须保留专用网格及边缘处理。
    const toolbar = getComputedStyle(shell.querySelector(".agent-catalog-toolbar")!);
    expect(toolbar.display).toBe("grid");
    expect(toolbar.borderRadius).toBe("0");
    expect(toolbar.borderTopWidth).toBe("0px");
    expect(getComputedStyle(shell.querySelector(".agent-skill-page-body")!).display).toBe("flex");
    expect(getComputedStyle(shell.querySelector(".skill-list-filter-row")!).gridTemplateColumns).toBe("minmax(240px, 1fr) auto 144px");
    // list is a scrolling region, whose full height must not become a surrounding card.
    // list 是滚动区域，其完整高度不能形成外围卡片。
    const list = getComputedStyle(shell.querySelector(".agent-skill-card-list")!);
    expect(list.backgroundColor).toBe("rgba(0, 0, 0, 0)");
    expect(list.borderTopStyle).toBe("");
  } finally { shell.remove(); style.remove(); }
});
