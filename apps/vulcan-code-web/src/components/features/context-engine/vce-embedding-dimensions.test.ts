import { describe, expect, it } from "vitest";
import type { PublicAiEmbeddingCapabilities } from "@/types";
import { preferredVceDimension, vceDimensionDomain, vceDimensionSupported } from "./vce-embedding-dimensions";

// Build one explicit host-compatible capability fixture; overrides describe individual boundary cases.
// 构建一个明确宿主兼容能力夹具；覆盖字段描述各边界场景。
// overrides contains declared fields; returns the full embedding capability without a fake default.
// overrides 包含声明字段；返回完整向量能力，不虚构默认值。
function capabilities(overrides: Partial<PublicAiEmbeddingCapabilities> = {}): PublicAiEmbeddingCapabilities {
  return { tasks: ["provider_default"], output_kinds: ["dense"], encodings: ["float"], dimensions: [],
    normalization: "l2", distance_metric: "cosine", ...overrides };
}

describe("explicit ContextEngine dimensions", () => {
  // Unknown/default-only declarations cannot authorize a runtime dimension.
  // 未知或仅默认值声明不能授权运行时维度。
  it("rejects provider-default-only and malformed domains", () => {
    for (const overrides of [{}, { default_dimensions: 1024 }, { dimensions: [0, 1024] },
      { dimensions: [1.5] }, { dimensions: [NaN] }, { minimum_dimensions: 2048, maximum_dimensions: 1024 },
      { dimensions: [512], minimum_dimensions: 1024 }, { maximum_dimensions: 4_294_967_296 }]) {
      expect(vceDimensionDomain(capabilities(overrides))).toBeNull();
    }
  });
  // All discrete bounds apply before preference, and user intent wins whenever still legal.
  // 离散边界先于偏好全部应用；用户意图仍合法时优先保留。
  it("preserves exact intent then chooses 1024, nearest above, or highest below", () => {
    const domain = vceDimensionDomain(capabilities({ dimensions: [4096, 512, 1024, 1024, 2048], maximum_dimensions: 2048 }));
    expect(domain).toEqual({ kind: "discrete", values: [512, 1024, 2048] });
    if (!domain) throw new Error("fixture must have explicit dimensions");
    expect(preferredVceDimension(domain)).toBe(1024);
    expect(preferredVceDimension(domain, 512)).toBe(512);
    expect(preferredVceDimension(domain, 4096)).toBe(1024);
    expect(preferredVceDimension({ kind: "discrete", values: [256, 1536, 2048] })).toBe(1536);
    expect(preferredVceDimension({ kind: "discrete", values: [256, 768] })).toBe(768);
    expect(vceDimensionSupported(domain, 1536)).toBe(false);
    expect(vceDimensionSupported(domain, 0)).toBe(false);
  });
  // One-sided declared ranges retain shared u32 boundaries and reject fractional user inputs.
  // 单侧声明范围保留共享 u32 边界并拒绝小数用户输入。
  it("supports explicit ranges without consulting provider defaults", () => {
    const domain = vceDimensionDomain(capabilities({ minimum_dimensions: 1536, default_dimensions: 128 }));
    expect(domain).toEqual({ kind: "range", minimum: 1536, maximum: 4_294_967_295 });
    if (!domain) throw new Error("fixture must declare a range");
    expect(preferredVceDimension(domain)).toBe(1536);
    expect(preferredVceDimension(domain, 2048)).toBe(2048);
    expect(vceDimensionSupported(domain, 1536.5)).toBe(false);
    expect(preferredVceDimension({ kind: "range", minimum: 1, maximum: 768 })).toBe(768);
  });
});
