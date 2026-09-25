// Verify that desktop discovery cannot silently switch to another Core after bridge failure.
// 验证桌面发现不会在桥失败后静默切换到另一 Core。
import { afterEach, describe, expect, it, vi } from "vitest";
import { readElectronServiceConnection } from "@/host-shell/electron-bridge";
import { resolveReachableBrowserServiceBaseUrl, resolveBrowserServiceAccessToken } from "./browser-service-discovery";

vi.mock("@/host-shell/electron-bridge", () => ({ readElectronServiceConnection: vi.fn() }));

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetAllMocks();
});

describe("Electron instance selection", () => {
  it("uses the GUI-selected instance without probing another port", async () => {
    // A competing query value and cached endpoint must not override the native owner.
    // 竞争的查询值和缓存端点不能覆盖原生所有者。
    const fetch = vi.fn();
    vi.stubGlobal("fetch", fetch);
    vi.stubGlobal("window", {
      location: { search: "?service=http://127.0.0.1:17999" },
      localStorage: { getItem: () => "http://127.0.0.1:17998", setItem: vi.fn() },
      sessionStorage: { setItem: vi.fn(), removeItem: vi.fn() },
      dispatchEvent: vi.fn(),
    });
    vi.mocked(readElectronServiceConnection).mockResolvedValue({
      baseUrl: "http://127.0.0.1:17921", pid: 42, startedAt: "123", serviceToken: "isolated-test-token", status: "ready",
    });
    expect(await resolveReachableBrowserServiceBaseUrl()).toBe("http://127.0.0.1:17921");
    expect(resolveBrowserServiceAccessToken()).toBe("isolated-test-token");
    expect(fetch).not.toHaveBeenCalled();
  });

  it("surfaces bridge failure without falling through to browser discovery", async () => {
    const fetch = vi.fn();
    vi.stubGlobal("fetch", fetch);
    vi.mocked(readElectronServiceConnection).mockRejectedValue(new Error("Selected Core is unavailable"));
    await expect(resolveReachableBrowserServiceBaseUrl()).rejects.toThrow("Selected Core is unavailable");
    expect(fetch).not.toHaveBeenCalled();
  });
});
