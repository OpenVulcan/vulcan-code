import { expect, it } from "vitest";
import wire from "./__fixtures__/plugin-prewarm-policy-wire.json";
import { parsePluginPrewarmPolicyReceipt, parsePluginPrewarmPolicyStatus } from "./plugin-prewarm-policy-contract";

// Rust emits this full fixture from real policy RPC responses and verifies it again without regeneration.
// Rust 从真实策略 RPC 响应产出此完整夹具，并在不重新生成时再次核验。
it("decodes actual publication and retains historical parent identity after restart", () => {
  expect(parsePluginPrewarmPolicyStatus(wire.initial)).toEqual(wire.initial);
  expect(parsePluginPrewarmPolicyStatus(wire.published)).toEqual(wire.published);
  expect(parsePluginPrewarmPolicyReceipt(wire.published.update)).toEqual(wire.published.update);
  expect(parsePluginPrewarmPolicyStatus({ ...wire.published, runtimeId: "restarted", revision: "new-live-revision" }).update?.request.runtimeId)
    .toBe(wire.published.update.request.runtimeId);
  expect(parsePluginPrewarmPolicyReceipt({ ...wire.published.update, phase: "publishing", persistenceError: "confirmation lost" }).persistenceError)
    .toBe("confirmation lost");
});

// Missing fields, duplicate ownership and unsafe counts cannot become actionable policy in the browser.
// 缺失字段、重复归属及不安全计数不能成为浏览器中可操作策略。
it("rejects malformed rules and contradictory transaction projections", () => {
  // The exact published declaration supplies a complete rule for single-field invalid mutations.
  // 精确已发布声明为单字段非法变更提供完整规则。
  const rule = wire.published.rules.find((entry) => entry.toolId === "control_tool");
  expect(rule, "Rust fixture must retain the control tool declaration").toBeDefined();
  for (const targetIdle of [0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1, "1", null]) {
    expect(() => parsePluginPrewarmPolicyStatus({ ...wire.published, rules: [{ ...rule, targetIdle }] })).toThrow("targetIdle");
  }
  for (const rules of [undefined, null, {}, [rule, { ...rule, pluginId: "different" }]]) {
    expect(() => parsePluginPrewarmPolicyStatus({ ...wire.published, rules })).toThrow("rules");
  }
  expect(() => parsePluginPrewarmPolicyStatus({ ...wire.published, capacityUpdateRetained: true })).toThrow("conflicting");
  expect(() => parsePluginPrewarmPolicyStatus({ ...wire.initial, update: undefined })).toThrow("receipt");
  expect(() => parsePluginPrewarmPolicyReceipt({ ...wire.published.update, phase: "ready" })).toThrow("phase");
  expect(() => parsePluginPrewarmPolicyReceipt({ ...wire.published.update, error: undefined })).toThrow("error");
  expect(() => parsePluginPrewarmPolicyReceipt({ ...wire.published.update, persistenceError: null })).toThrow("persistenceError");
  expect(() => parsePluginPrewarmPolicyReceipt({ ...wire.published.update, request: { ...wire.published.update.request, rules: undefined } })).toThrow("rules");
});
