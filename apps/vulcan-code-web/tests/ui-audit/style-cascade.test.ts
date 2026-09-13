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
