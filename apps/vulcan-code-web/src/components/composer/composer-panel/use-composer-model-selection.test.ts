import { describe, expect, it } from "vitest";

import { shouldDeferServiceModelChoiceSync } from "./use-composer-model-selection";

describe("composer model selection synchronization", () => {
  // Verifies a stale same-session service snapshot cannot overwrite an in-flight optimistic choice.
  // 验证同会话旧服务快照不能覆盖正在持久化的乐观选择。
  it("defers same-session snapshots while model persistence is pending", () => {
    expect(shouldDeferServiceModelChoiceSync(false, "provider/model/connection")).toBe(true);
  });

  // Verifies navigation immediately adopts the target conversation's persisted model.
  // 验证会话导航会立即采用目标会话的持久化模型。
  it("does not defer synchronization after the conversation scope changes", () => {
    expect(shouldDeferServiceModelChoiceSync(true, "provider/model/connection")).toBe(false);
  });

  // Verifies ordinary authoritative refreshes still synchronize after persistence settles.
  // 验证持久化结束后的普通权威刷新仍会同步。
  it("does not defer synchronization without a pending model request", () => {
    expect(shouldDeferServiceModelChoiceSync(false, null)).toBe(false);
  });
});
