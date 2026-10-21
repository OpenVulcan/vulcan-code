// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { McpServiceSettingsArea } from "./mcp-service-settings-area";
import { emptySessionGrant } from "./mcp-service-model";
import { loadMcpService, rotateMcpClientToken, setMcpServiceEnabled } from "@/service/mcp-service-service";
import type { McpServiceSnapshot } from "@/types";

vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t: (key: string) => key }) };
});
vi.mock("@/service/mcp-service-service", () => ({
  loadMcpService: vi.fn(),
  setMcpServiceEnabled: vi.fn(),
  createMcpClient: vi.fn(),
  updateMcpClient: vi.fn(),
  rotateMcpClientToken: vi.fn(),
  deleteMcpClient: vi.fn(),
}));

// host and root isolate the page from real services.
// host 和 root 将页面与真实服务隔离。
let host: HTMLDivElement;
let root: Root;

function snapshot(enabled: boolean): McpServiceSnapshot {
  return {
    enabled,
    endpointUrl: "http://127.0.0.1:17600/mcp",
    clients: [{
      id: "client-1",
      name: "Laptop",
      enabled: true,
      tokenHint: "wxyz",
      createdAtMs: 1,
      sessions: emptySessionGrant(),
      mediaTargets: [],
      callCount: 0,
    }],
    mediaCandidates: [],
    limits: { maxClients: 16, maxNameChars: 64, maxProjects: 64, maxMediaTargets: 32 },
  };
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.clearAllMocks();
  vi.mocked(loadMcpService).mockResolvedValue(snapshot(false));
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it("lists clients and turns the endpoint on through the service", async () => {
  vi.mocked(setMcpServiceEnabled).mockResolvedValue(snapshot(true));
  await act(async () => root.render(<McpServiceSettingsArea projects={[]} />));

  expect(host.querySelector("[data-mcp-client='client-1']")).not.toBeNull();
  expect(host.querySelector("[data-mcp-endpoint]")?.textContent).toBe("http://127.0.0.1:17600/mcp");
  await act(async () => (host.querySelector("[data-mcp-enabled]") as HTMLButtonElement).click());

  expect(setMcpServiceEnabled).toHaveBeenCalledWith(true);
});

// A rotated token is shown once after confirmation and is gone from the page after the dialog closes.
// 轮换的令牌在确认后展示一次，关闭对话框后不再出现在页面上。
it("shows a rotated token once and forgets it on close", async () => {
  vi.mocked(rotateMcpClientToken).mockResolvedValue({ snapshot: snapshot(false), clientId: "client-1", token: "vmcp_rotated" });
  await act(async () => root.render(<McpServiceSettingsArea projects={[]} />));

  const rotate = [...host.querySelectorAll("button")].find((button) => button.getAttribute("aria-label") === "mcpService.client.rotate")!;
  await act(async () => rotate.click());
  expect(rotateMcpClientToken).not.toHaveBeenCalled();
  await act(async () => (document.querySelector("[data-mcp-confirm]") as HTMLButtonElement).click());

  expect(rotateMcpClientToken).toHaveBeenCalledWith("client-1");
  expect(document.querySelector("[data-mcp-token]")?.textContent).toBe("vmcp_rotated");
  await act(async () => (document.querySelector("[data-mcp-token-done]") as HTMLButtonElement).click());
  expect(document.body.textContent).not.toContain("vmcp_rotated");
});
