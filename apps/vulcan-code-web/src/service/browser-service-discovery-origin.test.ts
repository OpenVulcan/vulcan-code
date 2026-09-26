// Verify actual discovery requests are restricted to the Vite development origin.
// 验证实际服务发现请求仅限 Vite 开发来源。
import { afterEach, describe, expect, it, vi } from "vitest";
import { readElectronServiceConnection } from "@/host-shell/electron-bridge";
import { clearBrowserServiceEndpointCache, resolveReachableBrowserServiceBaseUrl } from "./browser-service-discovery";
import { VULCAN_DEV_WEBUI_ORIGIN } from "./vite-service-discovery-origin";

vi.mock("@/host-shell/electron-bridge", () => ({ readElectronServiceConnection: vi.fn() }));

afterEach(() => {
  clearBrowserServiceEndpointCache();
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
  vi.resetAllMocks();
});

describe("Vite-only discovery requests", () => {
  it.each([
    { dev: true, origin: VULCAN_DEV_WEBUI_ORIGIN, discovery: true },
    { dev: false, origin: VULCAN_DEV_WEBUI_ORIGIN, discovery: false },
    { dev: true, origin: "http://127.0.0.1:17921", discovery: false },
    { dev: false, origin: "http://127.0.0.1:17921", discovery: false },
    { dev: true, origin: "http://127.0.0.1:17891", discovery: false },
    { dev: false, origin: "http://127.0.0.1:17891", discovery: false },
    { dev: true, origin: VULCAN_DEV_WEBUI_ORIGIN, discovery: false, selected: true },
  ])("dev=$dev origin=$origin selected=$selected requests discovery=$discovery", async ({ dev, origin, discovery, selected }) => {
    // Exercise the complete resolver, including health checks and a repeated connection attempt.
    // 执行完整解析流程，包含健康检查和再次连接。
    const endpoint = "http://127.0.0.1:17921";
    vi.stubEnv("DEV", dev);
    vi.stubEnv("VITE_VULCAN_SERVICE_URL", undefined);
    vi.mocked(readElectronServiceConnection).mockResolvedValue(null);
    vi.stubGlobal("window", {
      location: new URL(selected ? `${origin}/?service=${endpoint}` : origin),
      localStorage: { getItem: () => null, setItem: vi.fn() },
      sessionStorage: { setItem: vi.fn(), removeItem: vi.fn() },
      dispatchEvent: vi.fn(),
      setTimeout,
      clearTimeout,
    });
    // Record the request URLs; only the fixture service answers health probes.
    // 记录请求地址，仅夹具服务响应健康探测。
    const fetch = vi.fn(async (url: string) => {
      if (url === "/vulcan-service-discovery.json" || url === `${endpoint}/health`) {
        return Response.json({ status: "ok", baseUrl: endpoint });
      }
      return new Response(null, { status: 404 });
    });
    vi.stubGlobal("fetch", fetch);
    for (let attempt = 0; attempt < 2; attempt += 1) {
      expect(await resolveReachableBrowserServiceBaseUrl()).toBe(endpoint);
      clearBrowserServiceEndpointCache();
    }
    expect(fetch.mock.calls.filter(([url]) => url === "/vulcan-service-discovery.json"))
      .toHaveLength(discovery ? 2 : 0);
    expect(fetch.mock.calls.filter(([url]) => url === `${endpoint}/health`)).toHaveLength(2);
  });
});
