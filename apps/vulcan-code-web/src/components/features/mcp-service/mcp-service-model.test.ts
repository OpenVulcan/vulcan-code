import { describe, expect, it } from "vitest";

import type { McpServiceClientSummary } from "@/types";
import {
  connectionSnippet,
  emptySessionGrant,
  grantSummaryParts,
  MCP_CODEX_TOKEN_ENV,
  MCP_SERVER_NAME,
  withPermission,
  withRead,
} from "./mcp-service-model";

// keyTranslator returns stable keys so the assertions never depend on locale text.
// keyTranslator 返回稳定键名，使断言不依赖具体语言文案。
const keyTranslator = (key: string) => key;
const keyFormatter = (template: string, values: Record<string, string | number>) => `${template}(${Object.values(values).join(",")})`;

function client(overrides: Partial<McpServiceClientSummary>): McpServiceClientSummary {
  return {
    id: "client-1",
    name: "Client",
    enabled: true,
    tokenHint: "abcd",
    createdAtMs: 1,
    sessions: emptySessionGrant(),
    mediaTargets: [],
    callCount: 0,
    ...overrides,
  };
}

describe("session grant helpers", () => {
  // Turning read off removes every permission that depends on it, so the saved grant stays valid.
  // 关闭读取会移除所有依赖它的权限，使保存的授权保持有效。
  it("clears write permissions when read is turned off", () => {
    const full = { ...emptySessionGrant(), send: true, create: true, stop: true, answerQuestions: true };
    const next = withRead(full, false);

    expect(next).toMatchObject({ read: false, send: false, create: false, stop: false, answerQuestions: false });
    expect(next.projects).toEqual(full.projects);
  });

  // Turning a write permission on also grants read; turning it off leaves read alone.
  // 开启写权限同时开启读取；关闭写权限不影响读取。
  it("implies read when a write permission is turned on", () => {
    const none = withRead(emptySessionGrant(), false);

    expect(withPermission(none, "send", true)).toMatchObject({ read: true, send: true });
    expect(withPermission({ ...emptySessionGrant(), stop: true }, "stop", false)).toMatchObject({ read: true, stop: false });
  });
});

describe("grantSummaryParts", () => {
  it("lists granted permissions, the project scope and the media count", () => {
    const parts = grantSummaryParts(client({
      sessions: { ...emptySessionGrant(), send: true, projects: { mode: "selected", projectIds: ["a", "b"] } },
      mediaTargets: [{ connectionId: "c", targetId: "t" }],
    }), keyTranslator as never, keyFormatter);

    expect(parts).toEqual([
      "mcpService.summary.read",
      "mcpService.summary.send",
      "mcpService.summary.projects(2)",
      "mcpService.summary.media(1)",
    ]);
  });

  it("names a client without session access", () => {
    const parts = grantSummaryParts(client({ sessions: withRead(emptySessionGrant(), false) }), keyTranslator as never, keyFormatter);

    expect(parts).toEqual(["mcpService.summary.noSessions"]);
  });
});

describe("connectionSnippet", () => {
  const url = "http://127.0.0.1:17600/mcp";
  const token = "vmcp_secret";

  it("renders a JSON config with the bearer header", () => {
    const parsed = JSON.parse(connectionSnippet("json", url, token));

    expect(parsed.mcpServers[MCP_SERVER_NAME]).toEqual({ type: "http", url, headers: { Authorization: `Bearer ${token}` } });
  });

  // The Codex snippet points at an environment variable, so the token never lands in config.toml.
  // Codex 片段指向环境变量，令牌不会写入 config.toml。
  it("keeps the token out of the Codex config", () => {
    const snippet = connectionSnippet("codex", url, token);

    expect(snippet).toContain(`bearer_token_env_var = "${MCP_CODEX_TOKEN_ENV}"`);
    expect(snippet).not.toContain(token);
  });

  it("passes the token as a header in the Claude Code command", () => {
    const snippet = connectionSnippet("claude", url, token);

    expect(snippet).toContain(`--transport http ${MCP_SERVER_NAME} ${url}`);
    expect(snippet).toContain(`Authorization: Bearer ${token}`);
  });
});
