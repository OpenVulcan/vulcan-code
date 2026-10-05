import { describe, expect, it } from "vitest";

import { conversationTerminalFloatingIdentity } from "@/components/conversation/terminal/conversation-read-only-terminal-surface";

describe("conversation read-only terminal surface", () => {
  // Verifies every AI terminal receives isolated floating geometry even when display names match.
  // 验证每个 AI 终端即使显示名称相同也会获得隔离的浮动几何。
  it("builds stable owner and terminal scoped floating identities", () => {
    expect(conversationTerminalFloatingIdentity("terminal-a", "session-a"))
      .toBe('["ai-terminal","session-a","terminal-a"]');
    expect(conversationTerminalFloatingIdentity("terminal-a", "session-a"))
      .not.toBe(conversationTerminalFloatingIdentity("terminal-a", "session-b"));
  });
});
