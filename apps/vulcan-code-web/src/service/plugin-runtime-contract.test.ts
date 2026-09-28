import { describe, expect, it } from "vitest";
import wire from "./__fixtures__/plugin-runtime-wire.json";
import { parseCapacityCatalog, parseCapacityConfig, parseCapacityReceipt, parseCapacityStatus, parsePluginRuntime } from "./plugin-runtime-contract";
import { createCapacityDraft, draftStillCurrent, readCapacityDraft } from "@/components/features/skills/plugin-capacity-draft";

describe("Rust-generated plugin runtime wire contract", () => {
  // The fixture is reproduced by real host RPC tests, including the policy token transition.
  // 夹具由真实宿主 RPC 测试重现，包括策略令牌转换。
  it("reads actual Rust responses and preserves a complete replacement without defaults", () => {
    // initial and catalog are independent checkpoints of the same native capacity.
    // initial 和 catalog 是同一原生容量的独立检查点。
    const initial = parseCapacityStatus(wire.initialCapacity);
    const catalog = parseCapacityCatalog(wire.catalog)!;
    const runtime = parsePluginRuntime(wire.runtime)!;
    expect(catalog.capacities).toHaveLength(1);
    expect(runtime.policyUpdate?.request.config).toEqual(catalog.capacities[0].snapshot.config);
    expect(runtime.policyUpdate?.request.expectedRevision).toBe(initial.policyRevision);
    expect(runtime.policyUpdate?.appliedRevision).toBe(catalog.capacities[0].policyRevision);
    expect(readCapacityDraft(createCapacityDraft(initial))).toEqual(wire.initialCapacity.snapshot.config);
    expect(parsePluginRuntime(null)).toBeNull();
    expect(parseCapacityCatalog(null)).toBeNull();
  });

  // Unknown read-only metadata is harmless; dropping a new config field from a replacement is not.
  // 未知只读元数据无害；从替换配置丢弃新字段则不可接受。
  it("rejects unknown complete-config fields while allowing new read-only metadata", () => {
    expect(() => parseCapacityConfig({ ...wire.initialCapacity.snapshot.config, future_limit: 1 })).toThrow("unknown configuration");
    expect(() => parseCapacityConfig({ ...wire.initialCapacity.snapshot.config, resources: {
      ...wire.initialCapacity.snapshot.config.resources, future_limit: 1,
    } })).toThrow("unknown configuration");
    expect(parseCapacityStatus({ ...wire.initialCapacity, future_metadata: 1 })).toEqual(parseCapacityStatus(wire.initialCapacity));
  });

  // Required nullable fields must not accidentally become aliases for missing properties.
  // 必填可空字段不得意外成为缺失属性的别名。
  it("rejects missing nullable receipt and registration fields and mismatched owners", () => {
    expect(() => parseCapacityReceipt({ ...wire.runtime.policyUpdate, request: { ...wire.runtime.policyUpdate.request, config: undefined } })).toThrow();
    expect(() => parseCapacityReceipt({ ...wire.runtime.policyUpdate, error: undefined })).toThrow();
    expect(() => parseCapacityStatus({ ...wire.initialCapacity, managementRequest: undefined })).toThrow();
    expect(() => parseCapacityStatus({ ...wire.initialCapacity, runtimeId: "other-runtime" })).toThrow("owner mismatch");
    expect(() => parseCapacityCatalog({ ...wire.catalog, capacities: [...wire.catalog.capacities, ...wire.catalog.capacities] })).toThrow("duplicate");
    expect(() => parseCapacityCatalog({ ...wire.catalog, runtimeId: "other-runtime" })).toThrow("mismatched");
  });

  // JSON already rounds large integers; reject that observation rather than permit a rounded write.
  // JSON 已会舍入大整数；拒绝该观测，避免允许舍入后写入。
  it.each([Number.MAX_SAFE_INTEGER + 1, -1, 1.5, Infinity, "4", null])("rejects unsafe counters or limits: %s", (value) => {
    expect(() => parseCapacityConfig({ ...wire.initialCapacity.snapshot.config, max_queued_bytes: value })).toThrow();
    expect(() => parseCapacityStatus({ ...wire.initialCapacity, preparingMembers: value })).toThrow();
  });

  // Drafts must neither retarget a replacement owner nor replace an original policy predecessor after refresh.
  // 草稿不得改指替代所有者，也不得在刷新后替换原策略前驱。
  it("keeps original draft identity across policy, parent, capacity and selection changes", () => {
    // status is an actual original checkpoint; draft stays unchanged through every later observation.
    // status 为真实原检查点；后续每次观测均不改变 draft。
    const status = parseCapacityStatus(wire.initialCapacity);
    const draft = createCapacityDraft(status);
    expect(draftStillCurrent(draft, status)).toBe(true);
    expect(draftStillCurrent(draft, parseCapacityCatalog(wire.catalog)!.capacities[0])).toBe(false);
    expect(draftStillCurrent(draft, { ...status, runtimeId: "new-parent" })).toBe(false);
    expect(draftStillCurrent(draft, { ...status, snapshot: { ...status.snapshot, capacity_id: "replacement" } })).toBe(false);
    expect(draftStillCurrent(draft, { ...status, purpose: "hook" })).toBe(false);
    expect(draftStillCurrent(draft, { ...status, retirementRequested: true })).toBe(false);
    expect(draftStillCurrent(draft, undefined)).toBe(false);
    expect(draft.predecessor.expectedRevision).toBe(wire.initialCapacity.policyRevision);
  });

  // Decimal-only input avoids accidental zero, exponent or rounding conversions before native validation.
  // 仅十进制输入避免原生校验前发生意外零值、指数或舍入转换。
  it.each(["", " ", "-1", "1e2", "1.5", "9007199254740993"])("rejects invalid draft input %j", (value) => {
    // draft preserves the invalid text so the user can correct it explicitly.
    // draft 保留无效文本，供用户明确修正。
    const draft = createCapacityDraft(parseCapacityStatus(wire.initialCapacity));
    draft.numbers.max_queued_bytes = value;
    expect(() => readCapacityDraft(draft)).toThrow();
    expect(draft.numbers.max_queued_bytes).toBe(value);
  });

  // Shared minimums are rejected, never auto-zeroed; boundary ordering follows the native config.
  // 公共预留下限不合法时拒绝，绝不自动归零；边界顺序遵循原生配置。
  it("enforces native bounds without changing form input", () => {
    // draft starts from the real effective policy, not a test-invented default.
    // draft 从真实生效策略开始，而非测试虚构默认值。
    const draft = createCapacityDraft(parseCapacityStatus(wire.initialCapacity));
    draft.numbers.min_resident_vms = "1";
    expect(() => readCapacityDraft(draft)).toThrow("bounds");
    expect(draft.numbers.min_resident_vms).toBe("1");
    draft.kind = "dedicated";
    expect(readCapacityDraft(draft).resources.min_resident_vms).toBe(1);
    draft.numbers.max_resident_vms = "0";
    expect(() => readCapacityDraft(draft)).toThrow("bounds");
    draft.numbers.max_resident_vms = "1";
    expect(() => readCapacityDraft(draft)).toThrow("bounds");
  });
});
