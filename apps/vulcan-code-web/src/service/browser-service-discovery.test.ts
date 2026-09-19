// Tests for browser service candidate planning and explicit-selection pinning.
// 浏览器服务候选规划与明确选择固定的测试。

import { describe, expect, it } from "vitest";

import { planBrowserServiceCandidates, type BrowserServiceCandidateSources } from "./browser-service-discovery";

// buildSources returns a complete source set with every field explicitly provided.
// buildSources 返回字段全部显式给出的完整来源集合。
function buildSources(overrides: Partial<BrowserServiceCandidateSources> = {}): BrowserServiceCandidateSources {
  return {
    fromQuery: null,
    fromEnv: null,
    fromLocation: null,
    fromDiscovery: null,
    fromStorage: null,
    fromDefault: "http://127.0.0.1:17891",
    rangeCandidates: ["http://127.0.0.1:17892", "http://127.0.0.1:17893"],
    ...overrides,
  };
}

describe("planBrowserServiceCandidates", () => {
  it("pins an explicit query selection and offers no fallback candidates", () => {
    // An explicit selection must never silently fall through to another instance.
    // 明确选择绝不能静默回退到另一个实例。
    const plan = planBrowserServiceCandidates(
      buildSources({
        fromQuery: "http://127.0.0.1:17921",
        fromLocation: "http://127.0.0.1:17891",
        rangeCandidates: ["http://127.0.0.1:17892"],
      }),
    );

    expect(plan.pinnedBaseUrl).toBe("http://127.0.0.1:17921");
    expect(plan.candidates).toEqual([]);
  });

  it("prefers the explicit selection over every discovered source", () => {
    // Priority follows the documented order: explicit selection wins over discovery.
    // 优先级遵循文档顺序：明确选择优先于发现结果。
    const plan = planBrowserServiceCandidates(
      buildSources({
        fromQuery: "http://127.0.0.1:17930",
        fromEnv: "http://127.0.0.1:17931",
        fromDiscovery: "http://127.0.0.1:17932",
      }),
    );

    expect(plan.pinnedBaseUrl).toBe("http://127.0.0.1:17930");
  });

  it("pins the build-time environment selection when no query selection exists", () => {
    // The build-time selection is equally authoritative.
    // 构建期选择同样具有权威性。
    const plan = planBrowserServiceCandidates(
      buildSources({ fromEnv: "http://127.0.0.1:17931", fromDiscovery: "http://127.0.0.1:17932" }),
    );

    expect(plan.pinnedBaseUrl).toBe("http://127.0.0.1:17931");
    expect(plan.candidates).toEqual([]);
  });

  it("rejects an unusable explicit selection instead of switching to another service", () => {
    // A malformed explicit value must keep every other instance out of the request path.
    // 非法显式值必须使其他实例都不进入请求路径。
    for (const selected of ["not-a-url", "", "http://user:secret@127.0.0.1:17921",
      "http://127.0.0.1:17921/other", "http://127.0.0.1:17921/?serviceToken=secret"]) {
      expect(() => planBrowserServiceCandidates(buildSources({
        fromQuery: selected, fromLocation: "http://127.0.0.1:17891",
      }))).toThrow("The explicitly selected browser service URL is invalid.");
    }
    expect(() => planBrowserServiceCandidates(buildSources({
      fromEnv: "ftp://127.0.0.1:17921", fromLocation: "http://127.0.0.1:17891",
    }))).toThrow("The explicitly selected browser service URL is invalid.");
  });

  it("orders fallback candidates by documented priority and deduplicates them", () => {
    // Same-origin discovery wins, then the cache, then the default, then the range scan.
    // 同源发现优先，其次是缓存、默认值和端口范围扫描。
    const plan = planBrowserServiceCandidates(
      buildSources({
        fromLocation: "http://127.0.0.1:17891/",
        fromDiscovery: "http://127.0.0.1:17891",
        fromStorage: "http://127.0.0.1:17892",
        rangeCandidates: ["http://127.0.0.1:17892", "http://127.0.0.1:17893"],
      }),
    );

    expect(plan.pinnedBaseUrl).toBeNull();
    expect(plan.candidates).toEqual([
      "http://127.0.0.1:17891",
      "http://127.0.0.1:17892",
      "http://127.0.0.1:17893",
    ]);
  });

  it("drops non-http and unparsable sources entirely", () => {
    // Only http(s) loopback-capable origins are usable service endpoints.
    // 只有 http(s) 来源可作为服务端点。
    const plan = planBrowserServiceCandidates(
      buildSources({
        fromLocation: "file:///tmp/index.html",
        fromDiscovery: "   ",
        fromStorage: "ftp://127.0.0.1:17891",
        fromDefault: "http://127.0.0.1:17891",
        rangeCandidates: [],
      }),
    );

    expect(plan.candidates).toEqual(["http://127.0.0.1:17891"]);
  });
});
