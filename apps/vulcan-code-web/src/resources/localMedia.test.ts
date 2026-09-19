// @vitest-environment jsdom
// Verify that a legacy media URL is remapped to the selected Core asset route.
// 验证旧媒体 URL 被映射到选中 Core 的资源路由。
import { describe, expect, it, vi } from "vitest";
import { localMediaSourceUrl } from "./localMedia";

describe("legacy local media routes", () => {
  // The same encoded file identity must survive both historical Tauri URL forms.
  // 两种历史 Tauri URL 形式都必须保留同一编码文件身份。
  it("remaps both legacy video URLs to the current authenticated Core", () => {
    // Node 26 exposes an unusable experimental localStorage unless a file is configured.
    // Node 26 未配置文件时会暴露不可用的实验性 localStorage。
    const localValues = new Map([["vulcan.service.url", "http://127.0.0.1:17891"]]);
    const sessionValues = new Map([["vulcan.service.token", "fixture-token"]]);
    vi.stubGlobal("localStorage", { getItem: (key: string) => localValues.get(key) ?? null });
    vi.stubGlobal("sessionStorage", { getItem: (key: string) => sessionValues.get(key) ?? null });
    const encoded = "5pep6KeG6aKR5L6L5L6L5pelLm1wNA";
    const expected = `http://127.0.0.1:17891/asset/${encoded}?token=fixture-token`;
    try {
      expect(localMediaSourceUrl(`http://vulcan-media.localhost/asset/${encoded}`)).toBe(expected);
      expect(localMediaSourceUrl(`vulcan-media://localhost/asset/${encoded}`)).toBe(expected);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
