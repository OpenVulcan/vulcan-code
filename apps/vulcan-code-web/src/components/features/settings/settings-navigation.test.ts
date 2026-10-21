import { describe, expect, it } from "vitest";

import { normalizeSettingsSubArea, settingsSubAreaGroups } from "./settings-navigation";

// testTranslator returns stable keys so navigation-order assertions do not depend on locale text.
// testTranslator 返回稳定键名，使导航顺序断言不依赖具体语言文案。
const testTranslator = ((key: string) => key) as Parameters<typeof settingsSubAreaGroups>[0];

describe("settingsSubAreaGroups", () => {
  // Keeps the navigation to four groups of at most five pages each; MCP servers live under plugin management.
  // 保证导航为 4 个分组且每组最多 5 项；MCP 服务器位于插件管理下。
  it("keeps four groups with at most five pages each", () => {
    const groups = settingsSubAreaGroups(testTranslator);

    expect(groups.map((group) => group.id)).toEqual([
      "general",
      "models",
      "data",
      "about",
    ]);
    for (const group of groups) {
      expect(group.items.length).toBeGreaterThan(0);
      expect(group.items.length).toBeLessThanOrEqual(5);
    }
  });

  // The OpenVulcan account has its own page; other providers, accounts, models and function routing share one
  // merged model services page; subagent models keep their own page.
  // OpenVulcan 账户拥有独立页面；其他供应商、账号、模型与功能分配共用一个合并后的模型服务页；子代理模型有独立页面。
  it("shows the OpenVulcan account page, one model services page and the subagent models page in the models group", () => {
    const groups = settingsSubAreaGroups(testTranslator);
    const models = groups.find((group) => group.id === "models");

    expect(models?.items.map((item) => item.id)).toEqual(["openvulcan-account", "model-providers", "subagent-models"]);
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
    expect(normalizeSettingsSubArea("notifications")).toBe("notifications");
  });
});
