// Verify that a running service which refuses this page ends discovery with a visible, non-retryable error.
// 验证正在运行但拒绝当前页面的服务会以可见且不可重试的错误结束发现流程。
import { afterEach, describe, expect, it, vi } from "vitest";
import { readElectronServiceConnection } from "@/host-shell/electron-bridge";
import { resolveReachableBrowserServiceBaseUrl } from "./browser-service-discovery";
import { serviceBootstrapErrorIsRetryable } from "./webui-service-runtime";

vi.mock("@/host-shell/electron-bridge", () => ({ readElectronServiceConnection: vi.fn() }));
vi.mock("./webui-service-runtime", async () => {
  // Only the pure retry classifier is needed; the real module would construct service singletons.
  // 只需要纯函数重试判定；真实模块会构造服务单例。
  const source = await vi.importActual<typeof import("./webui-service-runtime")>("./webui-service-runtime");
  return { serviceBootstrapErrorIsRetryable: source.serviceBootstrapErrorIsRetryable };
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetAllMocks();
});

// stubBrowserWindow provides the minimal browser globals discovery reads, with no explicit service selection.
// stubBrowserWindow 提供发现流程读取的最小浏览器全局对象，且不含显式服务选择。
function stubBrowserWindow(): void {
  vi.stubGlobal("window", {
    location: { search: "", protocol: "http:", origin: "http://localhost:17601", port: "17601", hostname: "localhost" },
    localStorage: { getItem: () => null, setItem: vi.fn() },
    sessionStorage: { getItem: () => null, setItem: vi.fn(), removeItem: vi.fn() },
    dispatchEvent: vi.fn(),
    setTimeout: (callback: () => void, delay: number) => setTimeout(callback, delay),
    clearTimeout: (handle: ReturnType<typeof setTimeout>) => clearTimeout(handle),
  });
}

describe("browser service rejection", () => {
  it("reports a refusing service instead of treating it as absent", async () => {
    // Only the default port answers, and it refuses the page; every other probe finds nothing.
    // 只有默认端口有响应且拒绝当前页面；其余探测都找不到服务。
    stubBrowserWindow();
    vi.mocked(readElectronServiceConnection).mockResolvedValue(null);
    const fetch = vi.fn(async (url: string) => {
      if (url === "http://127.0.0.1:17921/health") {
        return new Response("forbidden", { status: 403 });
      }
      throw new TypeError("Failed to fetch");
    });
    vi.stubGlobal("fetch", fetch);
    const failure = await resolveReachableBrowserServiceBaseUrl().then(() => null, (error: unknown) => error);
    expect(String(failure)).toContain("rejected this page with HTTP 403");
    expect(serviceBootstrapErrorIsRetryable(failure)).toBe(false);
  });

  it("keeps an absent service retryable", async () => {
    stubBrowserWindow();
    vi.mocked(readElectronServiceConnection).mockResolvedValue(null);
    vi.stubGlobal("fetch", vi.fn(async () => {
      throw new TypeError("Failed to fetch");
    }));
    const failure = await resolveReachableBrowserServiceBaseUrl().then(() => null, (error: unknown) => error);
    expect(serviceBootstrapErrorIsRetryable(failure)).toBe(true);
  });
});
