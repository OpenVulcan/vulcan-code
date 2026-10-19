// @vitest-environment jsdom
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
  const root = postcss.parse(readFileSync(resolve(name === "skill-cards" ? "src/ui-kit/styles/app" : "src/ui-kit/styles/features", `${name}.css`), "utf8"));
  return root.nodes.filter((node: { type: string }) => node.type === "rule").map((node: { toString(): string }) => node.toString()).join("\n");
}

it("keeps the gutter reasoning tag absolute and no taller than the shortest tool row", () => {
  // rules are the kit's real top-level conversation rules; viewport and container branches are excluded.
  // rules 是套件真实的会话顶层规则，排除视口与容器分支。
  const rules = postcss.parse(readFileSync(resolve("src/ui-kit/styles/conversation.css"), "utf8")).nodes
    .filter((node: { type: string }) => node.type === "rule");
  // declaration returns the last value a rule listing the given selector declares for the property.
  // declaration 返回列出给定选择器的规则为该属性声明的最后取值。
  const declaration = (selector: string, property: string): string => {
    const values = rules.filter((node: { selectors: string[] }) => node.selectors.includes(selector))
      .flatMap((node: { nodes: { prop: string; value: string }[] }) => node.nodes.filter((decl) => decl.prop === property).map((decl) => decl.value));
    expect(values.length, `${selector} declares no ${property}`).toBeGreaterThan(0);
    return values[values.length - 1];
  };
  // units reads a kit length written as calc(var(--vk-u) * N), so both sides share one scale.
  // units 读取写作 calc(var(--vk-u) * N) 的套件长度，使两侧使用同一刻度。
  const units = (value: string): number => {
    const match = /^calc\(var\(--vk-u\) \* ([\d.]+)\)$/.exec(value.trim());
    expect(match, `expected a --vk-u multiple, got ${value}`).not.toBeNull();
    return Number.parseFloat(match![1]);
  };
  expect(declaration(".vk-flow[data-think] > .vk-think-tag", "position")).toBe("absolute");
  expect(declaration(".vk-gutter-anchor > .vk-think-tag", "position")).toBe("absolute");
  expect(units(declaration(".vk-think-tag[data-compact]", "height"))).toBeLessThanOrEqual(units(declaration(".vk-tool-head", "min-height")));
  expect(units(declaration(".vk-gutter-anchor", "min-height"))).toBeGreaterThanOrEqual(units(declaration(".vk-think-tag[data-compact]", "height")));
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
  shell.innerHTML = '<div class="skill-page-body agent-skill-page-body"><div class="vk-page-toolbar agent-catalog-toolbar"></div><div class="skill-card-list agent-skill-card-list"></div></div>';
  document.head.appendChild(style); document.body.appendChild(shell);
  try {
    // toolbar stays a fixed row above the scrolling list after loading the lazy stylesheet.
    // toolbar 在载入懒加载样式后仍是滚动列表上方的固定行。
    expect(getComputedStyle(shell.querySelector(".agent-catalog-toolbar")!).flexGrow).toBe("0");
    expect(getComputedStyle(shell.querySelector(".agent-skill-page-body")!).display).toBe("flex");
    // list is a scrolling region, whose full height must not become a surrounding card.
    // list 是滚动区域，其完整高度不能形成外围卡片。
    const list = getComputedStyle(shell.querySelector(".agent-skill-card-list")!);
    expect(list.backgroundColor).toBe("rgba(0, 0, 0, 0)");
    expect(list.borderTopStyle).toBe("");
  } finally { shell.remove(); style.remove(); }
});
