import { describe, expect, it } from "vitest";

import {
  serviceModelChoiceSyncValue,
  shouldDeferServiceModelChoiceSync,
} from "./use-composer-model-selection";

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

  // Verifies a same-session catalog refresh cannot replace the retained exact model with list order.
  // 验证同一会话目录刷新不能用列表顺序替换已保留的精确模型。
  it("preserves the current model when a same-session snapshot has no active row", () => {
    expect(serviceModelChoiceSyncValue(false, "")).toBeNull();
  });

  // Verifies an authoritative service selection always replaces the retained local value.
  // 验证后端权威选择始终替换本地保留值。
  it("adopts an exact service-selected account", () => {
    expect(serviceModelChoiceSyncValue(false, "kimi/k3/account")).toBe("kimi/k3/account");
  });

  // Verifies navigation cannot leak the previous conversation model into an unresolved session.
  // 验证会话切换不能把上一会话模型泄漏到尚未解析的会话。
  it("clears the previous model when scope changes without an active row", () => {
    expect(serviceModelChoiceSyncValue(true, "")).toBe("");
  });
});
