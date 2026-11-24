import { describe, expect, it } from "vitest";

import { normalizeSettingsSubArea, settingsSubAreaGroups } from "./settings-navigation";

// testTranslator returns stable keys so navigation-order assertions do not depend on locale text.
// testTranslator 返回稳定键名，使导航顺序断言不依赖具体语言文案。
const testTranslator = ((key: string) => key) as Parameters<typeof settingsSubAreaGroups>[0];

describe("settingsSubAreaGroups", () => {
  // Keeps the navigation to five groups of at most five pages each. MCP servers that Vulcan Code connects to live
  // under plugin management; the connections group holds the MCP service other tools connect to.
  // 保证导航为 5 个分组且每组最多 5 项。Vulcan Code 连接的 MCP 服务器位于插件管理下；外部连接分组承载供其他工具连接的 MCP 服务。
  it("keeps five groups with at most five pages each", () => {
    const groups = settingsSubAreaGroups(testTranslator);

    expect(groups.map((group) => group.id)).toEqual([
      "general",
      "models",
      "connections",
      "data",
      "about",
    ]);
    for (const group of groups) {
      expect(group.items.length).toBeGreaterThan(0);
      expect(group.items.length).toBeLessThanOrEqual(5);
    }
  });

  // Each model settings page should be reachable exactly once, regardless of menu order.
  // 模型设置的每个页面都应当有且仅有一个入口，不依赖菜单顺序。
  it("lists each account, model service, subagent and economy settings page once in the models group", () => {
    const groups = settingsSubAreaGroups(testTranslator);
    // Count matching page identities within the models group.
    // 在模型分组中统计与页面标识匹配的入口。
    const modelPages = groups.filter((group) => group.id === "models").flatMap((group) => group.items);
    for (const page of ["openvulcan-account", "model-providers", "subagent-models", "economy-mode"]) {
      expect(modelPages.filter((item) => item.id === page)).toHaveLength(1);
    }
  });

  // Exposes the visible acceptance surface from the about group.
  // 从关于分组公开可见的视觉验收页面。
  it("exposes the visual lab from the about group", () => {
    const groups = settingsSubAreaGroups(testTranslator);
    const about = groups.find((group) => group.id === "about");

    expect(about?.items.map((item) => item.id)).toContain("visual-lab");
  });
});

describe("normalizeSettingsSubArea", () => {
  // Retired ids keep working as deep links by resolving to their replacement page.
  // 已退役的标识通过解析到替代页面继续作为深链接使用。
  it("resolves retired page ids", () => {
    expect(normalizeSettingsSubArea("ai-web-search")).toBe("model-providers");
    expect(normalizeSettingsSubArea("ai-music-cover")).toBe("model-providers");
    expect(normalizeSettingsSubArea("ai-capabilities")).toBe("model-providers");
    expect(normalizeSettingsSubArea("credential-management")).toBe("model-providers");
    expect(normalizeSettingsSubArea("model-list")).toBe("model-providers");
    expect(normalizeSettingsSubArea("prompt-composer")).toBe("context-basics");
    expect(normalizeSettingsSubArea("changelog")).toBe("app-update");
    expect(normalizeSettingsSubArea("notifications")).toBe("notifications");
  });
});
