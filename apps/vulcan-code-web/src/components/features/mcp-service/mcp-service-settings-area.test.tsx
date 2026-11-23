// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { McpServiceSettingsArea } from "./mcp-service-settings-area";
import { emptySessionGrant } from "./mcp-service-model";
import { createMcpClient, loadMcpCallDetail, loadMcpCallLogs, loadMcpService, rotateMcpClientToken, setMcpServiceEnabled, updateMcpClient } from "@/service/mcp-service-service";
import type { McpServiceSnapshot } from "@/types";

vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t: (key: string) => key }) };
});
vi.mock("@/service/mcp-service-service", () => ({
  loadMcpService: vi.fn(),
  loadMcpCallLogs: vi.fn(),
  loadMcpCallDetail: vi.fn(),
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

// Verifies that recording is opt-in and that matching rows load their detail on demand.
// 验证日志必须显式开启，且匹配的记录仅在用户打开时加载详情。
it("opts into recording and queries a client's calls by project and session", async () => {
  const current = snapshot(true);
  vi.mocked(updateMcpClient).mockResolvedValue(current);
  vi.mocked(loadMcpCallLogs).mockResolvedValue({ rows: [{ id: 41, clientId: "client-1", clientName: "Laptop", projectId: "p_2", sid: "s_3", tool: "file_read", startedAt: 1, finishedAt: 2, status: "completed" }], nextCursor: null });
  vi.mocked(loadMcpCallDetail).mockResolvedValue({ arguments: { sid: "s_3" }, result: { text: "verified fixture" } });
  await act(async () => root.render(<McpServiceSettingsArea projects={[]} />));
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="mcpService.client.edit"]')!.click());
  const recordSwitch = document.querySelector<HTMLButtonElement>("[data-mcp-record-calls]")!;
  expect(recordSwitch.getAttribute("aria-checked")).toBe("false");
  await act(async () => recordSwitch.click());
  await act(async () => document.querySelector<HTMLButtonElement>("[data-mcp-client-save]")!.click());
  expect(updateMcpClient).toHaveBeenCalledWith(expect.objectContaining({ clientId: "client-1", recordCalls: true }));

  await act(async () => host.querySelector<HTMLButtonElement>("[data-mcp-client-logs]")!.click());
  expect(loadMcpCallLogs).toHaveBeenCalledWith({ clientId: "client-1", ascending: false });
  expect(loadMcpCallDetail).not.toHaveBeenCalled();
  const dialog = document.querySelector("[data-mcp-call-logs]")!;
  await act(async () => {
    // Update each controlled field through the same native input event as a user.
    // 通过与用户输入相同的原生事件更新受控字段。
    const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    const project = dialog.querySelector<HTMLInputElement>("[data-mcp-log-project]")!;
    setValue.call(project, "p_2");
    project.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => {
    const session = dialog.querySelector<HTMLInputElement>("[data-mcp-log-session]")!;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(session, "s_3");
    session.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => dialog.querySelector<HTMLButtonElement>('button[type="submit"]')!.click());
  expect(loadMcpCallLogs).toHaveBeenLastCalledWith({ clientId: "client-1", projectId: "p_2", sid: "s_3", ascending: false });
  await act(async () => dialog.querySelector<HTMLButtonElement>("[data-mcp-log-id='41']")!.click());
  expect(loadMcpCallDetail).toHaveBeenCalledWith(41);
  expect([...dialog.querySelectorAll("pre")].map((node) => JSON.parse(node.textContent!))).toEqual([{ sid: "s_3" }, { text: "verified fixture" }]);
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


// Saving a tool choice maintains separate project and library grants.
// 保存一个工具勾选时分别保留项目和知识库授权。
it("saves individual MCP tools and their independent scopes", async () => {
  const current = snapshot(false);
  current.toolCandidates = ["file_read", "shell_command"];
  current.knowledgeCandidates = { library: "Reference" };
  current.clients[0].toolIds = ["file_read"];
  vi.mocked(loadMcpService).mockResolvedValue(current);
  vi.mocked(updateMcpClient).mockResolvedValue(current);
  await act(async () => root.render(<McpServiceSettingsArea projects={[{ id: "project", name: "Project", path: "/project" }]} />));

  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="mcpService.client.edit"]')!.click());
  const fileRead = document.querySelector<HTMLButtonElement>('[data-mcp-tool="file_read"]')!;
  const shell = document.querySelector<HTMLButtonElement>('[data-mcp-tool="shell_command"]')!;
  expect(fileRead.getAttribute("aria-checked")).toBe("true");
  expect(shell.getAttribute("aria-checked")).toBe("false");
  await act(async () => shell.click());
  const toolScopeButtons = Array.from(document.querySelectorAll<HTMLButtonElement>("[data-mcp-workspace-grants] button"));
  await act(async () => toolScopeButtons.find((button) => button.textContent === "mcpService.field.scopeSelected")!.click());
  expect(document.querySelector<HTMLButtonElement>("[data-mcp-client-save]")!.disabled).toBe(true);
  await act(async () => document.querySelector<HTMLButtonElement>("[data-mcp-workspace-grants] [role=checkbox]")!.click());
  const libraryChecks = document.querySelectorAll<HTMLButtonElement>("[data-mcp-knowledge-grants] [role=checkbox]");
  await act(async () => libraryChecks[0].click());
  await act(async () => libraryChecks[1].click());
  await act(async () => document.querySelector<HTMLButtonElement>("[data-mcp-client-save]")!.click());

  expect(updateMcpClient).toHaveBeenCalledWith(expect.objectContaining({
    clientId: "client-1",
    toolIds: ["file_read", "shell_command"],
    toolProjectIds: ["project"],
    knowledgeLibraries: { library: true },
  }));
});

// Switching authorization pages retains draft choices and a group toggle affects only its own tools.
// 切换授权页保留草稿选项，分组开关只影响本组工具。
it("isolates session and media grants while toggling the file group", async () => {
  const current = snapshot(false);
  const target = { connectionId: "connection", targetId: "target" };
  current.mediaCandidates = [{ ...target, capability: "image_generate", model: "example/model", displayName: "Media", accountName: "Fixture" }];
  current.toolCandidates = ["list_projects", "send_message", "file_read", "file_write", "shell_command", "generate_media"];
  current.clients[0].toolIds = ["send_message", "file_read", "shell_command", "generate_media"];
  current.clients[0].mediaTargets = [target];
  vi.mocked(loadMcpService).mockResolvedValue(current);
  vi.mocked(updateMcpClient).mockResolvedValue(current);
  await act(async () => root.render(<McpServiceSettingsArea projects={[{ id: "p", name: "Project", path: "/project" }]} />));
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="mcpService.client.edit"]')!.click());

  const groupToggle = document.querySelector<HTMLButtonElement>('[data-mcp-tool-group="files"] [data-mcp-group-toggle]')!;
  expect(groupToggle.getAttribute("aria-checked")).toBe("mixed");
  expect(document.querySelector('[data-mcp-tool="send_message"]')).toBeNull();
  expect(document.querySelector('[data-mcp-tool="generate_media"]')).toBeNull();
  const fileRead = document.querySelector('[data-mcp-tool="file_read"]')!;
  expect(fileRead.getAttribute("aria-label")).toBe("mcpService.tool.file_read.name");
  expect(fileRead.closest("label")?.querySelector("small")?.textContent).toBe("mcpService.tool.file_read.detail");
  await act(async () => groupToggle.click());
  expect(groupToggle.getAttribute("aria-checked")).toBe("true");
  expect(document.querySelector('[data-mcp-tool="file_write"]')?.getAttribute("aria-checked")).toBe("true");
  await act(async () => groupToggle.click());
  expect(groupToggle.getAttribute("aria-checked")).toBe("false");
  expect(fileRead.getAttribute("aria-checked")).toBe("false");

  const tabs = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="tab"]'));
  await act(async () => tabs.find((tab) => tab.textContent === "mcpService.field.sessions")!.click());
  expect(document.querySelector('[data-mcp-project-scope]')).not.toBeNull();
  expect(document.querySelector('[data-mcp-tool="list_projects"]')?.getAttribute("aria-checked")).toBe("false");
  expect(document.querySelector('[data-mcp-tool="send_message"]')?.closest("label")?.querySelector("small")?.textContent).toBe("mcpService.grant.send");
  const scopeButtons = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-mcp-project-scope] button'));
  await act(async () => scopeButtons.find((button) => button.textContent === "mcpService.field.scopeSelected")!.click());
  expect(document.querySelector<HTMLButtonElement>("[data-mcp-client-save]")!.disabled).toBe(true);
  await act(async () => document.querySelector<HTMLButtonElement>('[data-mcp-project-scope] [role="checkbox"]')!.click());
  await act(async () => tabs.find((tab) => tab.textContent === "mcpService.tabs.media")!.click());
  expect(document.querySelector('[data-mcp-project-scope]')).toBeNull();
  expect(document.querySelector('[data-mcp-media-grants] [role="checkbox"]')?.getAttribute("aria-checked")).toBe("true");

  await act(async () => document.querySelector<HTMLButtonElement>("[data-mcp-client-save]")!.click());
  expect(updateMcpClient).toHaveBeenCalledWith(expect.objectContaining({
    toolIds: ["send_message", "shell_command", "generate_media"],
    sessions: expect.objectContaining({ read: true, send: true, projects: { mode: "selected", projectIds: ["p"] } }),
    mediaTargets: [target],
  }));
  expect(vi.mocked(updateMcpClient).mock.lastCall![0]).not.toHaveProperty("toolProjectIds");
});

// Editing only a model must preserve a stored project restriction, including an empty session scope.
// 仅修改模型时须保留已存的项目限制，包括空的会话项目范围。
it.each([{ ids: [] }, { ids: ["file_read"] }, { ids: ["list_sessions", "file_read"] }])(
  "saves a model directly from the media tab with existing grants $ids",
  async ({ ids }) => {
    const current = snapshot(true);
    const client = current.clients[0];
    client.toolIds = ids;
    client.sessions = { ...emptySessionGrant(), read: ids.includes("list_sessions"), projects: { mode: "selected", projectIds: [] } };
    client.toolProjectIds = ids.includes("file_read") ? ["p"] : [];
    const target = { connectionId: "account", targetId: "image" };
    current.mediaCandidates = [{ ...target, capability: "image_generate", model: "model", displayName: "Model", accountName: "Account" }];
    current.toolCandidates = [...ids, "generate_media"];
    vi.mocked(loadMcpService).mockResolvedValue(current);
    // Reopen the dialog from the snapshot returned by a successful settings update.
    // 从设置更新成功后返回的快照重新打开弹窗。
    vi.mocked(updateMcpClient).mockImplementation(async (request) => ({
      ...current, clients: [{ ...client, mediaTargets: request.mediaTargets }],
    }));
    await act(async () => root.render(<McpServiceSettingsArea projects={[]} />));
    const edit = host.querySelector<HTMLButtonElement>('[aria-label="mcpService.client.edit"]')!;
    for (const selected of [true, false]) {
      await act(async () => edit.click());
      await act(async () => document.querySelector<HTMLButtonElement>('[role="tab"][data-value="media"]')!.click());
      const checkbox = document.querySelector<HTMLButtonElement>('[data-mcp-media-grants] [role="checkbox"]')!;
      expect(checkbox.getAttribute("aria-checked")).toBe(String(!selected));
      await act(async () => checkbox.click());
      const save = document.querySelector<HTMLButtonElement>("[data-mcp-client-save]")!;
      expect(save.disabled).toBe(false);
      await act(async () => save.click());
      expect(updateMcpClient).toHaveBeenLastCalledWith(expect.objectContaining({
        clientId: client.id, sessions: client.sessions, toolIds: ids, mediaTargets: selected ? [target] : [],
      }));
      expect(vi.mocked(updateMcpClient).mock.lastCall![0]).not.toHaveProperty("toolProjectIds");
    }
  },
);

// New session permissions require a project within an existing empty selected scope.
// 新增会话授权仍需在已保存的空指定范围中选择项目。
it("requires a project before adding a session tool to a stored empty selected scope", async () => {
  const current = snapshot(true);
  current.clients[0].sessions.projects = { mode: "selected", projectIds: [] };
  current.clients[0].toolIds = ["list_sessions"];
  current.toolCandidates = ["list_sessions", "send_message"];
  vi.mocked(loadMcpService).mockResolvedValue(current);
  await act(async () => root.render(<McpServiceSettingsArea projects={[{ id: "p", name: "Project", path: "/p" }]} />));
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="mcpService.client.edit"]')!.click());
  await act(async () => document.querySelector<HTMLButtonElement>('[role="tab"][data-value="sessions"]')!.click());
  await act(async () => document.querySelector<HTMLButtonElement>('[data-mcp-tool="send_message"]')!.click());
  const save = document.querySelector<HTMLButtonElement>("[data-mcp-client-save]")!;
  expect(save.disabled).toBe(true);
  await act(async () => document.querySelector<HTMLButtonElement>('[data-mcp-project-scope] [role="checkbox"]')!.click());
  expect(save.disabled).toBe(false);
});

// A standalone client saves without session or multimodal access.
// 仅使用独立工具的客户端可在无会话或多模态权限时保存。
it("creates a standalone-only client without granting session operations", async () => {
  const current = snapshot(false);
  current.toolCandidates = ["list_projects", "file_read", "generate_media"];
  vi.mocked(loadMcpService).mockResolvedValue(current);
  vi.mocked(createMcpClient).mockResolvedValue({ snapshot: current, clientId: "new-client", token: "synthetic" });
  await act(async () => root.render(<McpServiceSettingsArea projects={[]} />));
  await act(async () => host.querySelector<HTMLButtonElement>("[data-mcp-add-client]")!.click());
  await act(async () => {
    const input = document.querySelector<HTMLInputElement>("#mcp-client-name")!;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "Tools only");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => document.querySelector<HTMLButtonElement>('[data-mcp-tool="file_read"]')!.click());
  expect(document.querySelector("[data-mcp-workspace-grants] [role=checkbox]")).toBeNull();
  await act(async () => document.querySelector<HTMLButtonElement>("[data-mcp-client-save]")!.click());

  expect(createMcpClient).toHaveBeenCalledWith(expect.objectContaining({
    toolIds: ["file_read"],
    toolProjectIds: [],
    sessions: { read: false, send: false, create: false, stop: false, answerQuestions: false, projects: { mode: "all" } },
    mediaTargets: [],
  }));
});

// An existing restriction persists in the editor until the unrestricted option is selected and saved.
// 编辑器保留已有范围限制，直到显式选择不限定项目并保存。
it("removes an existing tool restriction only on the explicit unrestricted choice", async () => {
  const current = snapshot(false);
  current.toolCandidates = ["file_read"];
  current.clients[0].toolIds = ["file_read"];
  current.clients[0].toolProjectIds = ["p"];
  vi.mocked(loadMcpService).mockResolvedValue(current);
  vi.mocked(updateMcpClient).mockResolvedValue(current);
  await act(async () => root.render(<McpServiceSettingsArea projects={[{ id: "p", name: "Project", path: "/project" }]} />));
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="mcpService.client.edit"]')!.click());

  const project = document.querySelector<HTMLButtonElement>("[data-mcp-workspace-grants] [role=checkbox]")!;
  expect(project.getAttribute("aria-checked")).toBe("true");
  await act(async () => project.click());
  const save = document.querySelector<HTMLButtonElement>("[data-mcp-client-save]")!;
  expect(save.disabled).toBe(true);
  const buttons = Array.from(document.querySelectorAll<HTMLButtonElement>("[data-mcp-workspace-grants] button"));
  await act(async () => buttons.find((button) => button.textContent === "mcpService.field.scopeUnrestricted")!.click());
  await act(async () => save.click());
  expect(updateMcpClient).toHaveBeenCalledWith(expect.objectContaining({ toolIds: ["file_read"], toolProjectIds: [] }));
});
