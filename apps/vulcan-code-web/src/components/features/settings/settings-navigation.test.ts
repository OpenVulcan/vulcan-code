import { describe, expect, it } from "vitest";

import { settingsSubAreaGroups } from "./settings-navigation";

// testTranslator returns stable keys so navigation-order assertions do not depend on locale text.
// testTranslator 返回稳定键名，使导航顺序断言不依赖具体语言文案。
const testTranslator = ((key: string) => key) as Parameters<typeof settingsSubAreaGroups>[0];

describe("settingsSubAreaGroups", () => {
  // Keeps search and extraction as the first two AI capability entries.
  // 保证网页搜索和网页抓取始终是 AI 能力分组的前两项。
  it("places web search before web extract", () => {
    const groups = settingsSubAreaGroups(testTranslator);
    const aiCapabilities = groups.find((group) => group.id === "ai-capabilities");

    expect(aiCapabilities?.items.slice(0, 2).map((item) => item.id)).toEqual([
      "ai-web-search",
      "ai-web-extract",
    ]);
  });
});
