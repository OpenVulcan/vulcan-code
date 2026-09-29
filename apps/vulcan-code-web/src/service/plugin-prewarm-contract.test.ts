import { expect, it } from "vitest";
import wire from "./__fixtures__/plugin-prewarm-wire.json";
import { parsePluginPrewarmCatalog, parsePluginPrewarmReceipt } from "./plugin-prewarm-contract";

// The fixture is emitted and compared by the real Rust RPC test, not hand-authored browser JSON.
// 夹具由真实 Rust RPC 测试产出并比较，不是手写浏览器 JSON。
it("decodes actual preparation and completion without changing native evidence", () => {
  expect(parsePluginPrewarmCatalog(wire.catalog)).toEqual(wire.catalog);
  expect(parsePluginPrewarmReceipt(wire.prepared)).toEqual(wire.prepared);
  expect(parsePluginPrewarmReceipt(wire.completed)).toEqual(wire.completed);
  expect(wire.completed.satisfied.ready).toBe(wire.completed.targetIdle);
});

// Generation identity must survive values beyond IEEE-754 integer precision and reject aliases.
// 代次身份必须保留超过 IEEE-754 整数精度的值，并拒绝别名形式。
it("preserves canonical u64 strings and rejects malformed or unsafe values", () => {
  expect(parsePluginPrewarmReceipt({ ...wire.prepared, pluginGeneration: "18446744073709551615" }).pluginGeneration).toBe("18446744073709551615");
  for (const pluginGeneration of [1, "01", "-1", "1.0", "18446744073709551616", " 1", "1e3"]) {
    expect(() => parsePluginPrewarmReceipt({ ...wire.prepared, pluginGeneration })).toThrow("pluginGeneration");
  }
  for (const targetIdle of [-1, Number.MAX_SAFE_INTEGER + 1, 1.5, "2"]) {
    expect(() => parsePluginPrewarmReceipt({ ...wire.prepared, targetIdle })).toThrow("targetIdle");
  }
  expect(() => parsePluginPrewarmReceipt({ ...wire.prepared, retired: undefined })).toThrow("retired");
  expect(() => parsePluginPrewarmReceipt({ ...wire.prepared, operation: undefined })).toThrow("operation");
  expect(() => parsePluginPrewarmReceipt({ ...wire.prepared, error: undefined })).toThrow("error");
});

// Mixed ownership and duplicate rows cannot authorize a UI action, even when individual shapes are valid.
// 即使单项形状有效，混合归属或重复行也不能授权界面操作。
it("rejects incoherent catalogs and malformed completion observations", () => {
  expect(() => parsePluginPrewarmCatalog({ ...wire.catalog, targets: [{ ...wire.prepared, runtimeId: "other" }] })).toThrow("owner mismatch");
  expect(() => parsePluginPrewarmCatalog({ ...wire.catalog, targets: [wire.prepared, wire.prepared] })).toThrow("duplicate invocationId");
  expect(() => parsePluginPrewarmCatalog({ ...wire.catalog, tools: [...wire.catalog.tools, ...wire.catalog.tools] })).toThrow("duplicate toolId");
  expect(() => parsePluginPrewarmCatalog({ ...wire.catalog, ownerSessionId: "unknown" })).toThrow("UUID");
  expect(() => parsePluginPrewarmReceipt({ ...wire.completed, satisfied: { ...wire.completed.satisfied, ready: Number.NaN } })).toThrow("ready");
  expect(() => parsePluginPrewarmReceipt({ ...wire.completed, completed: [null] })).toThrow("completed operation");
});
