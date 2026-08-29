import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

/// Reads one stylesheet relative to this production conversation component directory.
/// 读取相对于当前生产会话组件目录的一份样式表。
function readConversationStylesheet(relativeUrl: string): string {
  return readFileSync(fileURLToPath(new URL(relativeUrl, import.meta.url)), "utf8");
}

describe("conversation card scroll chaining", () => {
  // Expanded single cards, grouped cards, and thinking cards must hand boundary wheel deltas to the transcript.
  // 展开的单卡、聚合卡与思考卡必须把边界滚轮增量交给转录视口。
  it("keeps every bounded card viewport on automatic overscroll chaining", () => {
    const toolStyles = readConversationStylesheet("../../styles/tool-presentations.css");
    const specialStyles = readConversationStylesheet("../../styles/special-blocks.css");

    expect(toolStyles).toMatch(/\.tool-presentation-card[\s\S]*?overscroll-behavior:\s*auto;/u);
    expect(specialStyles).toMatch(/\.conversation-tool-sequence[\s\S]*?overscroll-behavior:\s*auto;/u);
    expect(specialStyles).toMatch(/\.thinking-body[\s\S]*?overscroll-behavior:\s*auto;/u);
  });
});
