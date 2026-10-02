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

  // Keeps providers, credentials, models and the merged multimodal extension page together under one group.
  // 将供应商、凭据、模型与合并后的多模态能力扩展页统一归入同一分组。
  it("groups providers, credentials, models and multimodal extensions", () => {
    const groups = settingsSubAreaGroups(testTranslator);
    const models = groups.find((group) => group.id === "models");

    expect(models?.items.map((item) => item.id)).toEqual([
      "model-providers",
      "credential-management",
      "model-list",
      "ai-capabilities",
    ]);
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
    expect(normalizeSettingsSubArea("ai-web-search")).toBe("ai-capabilities");
    expect(normalizeSettingsSubArea("ai-music-cover")).toBe("ai-capabilities");
    expect(normalizeSettingsSubArea("prompt-composer")).toBe("context-basics");
    expect(normalizeSettingsSubArea("notifications")).toBe("notifications");
  });
});
