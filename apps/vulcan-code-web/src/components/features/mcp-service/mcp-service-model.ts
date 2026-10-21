import type { TranslationKey } from "@/i18n-messages";
import type { McpMediaTargetEntry, McpServiceClientSummary, McpSessionGrantEntry } from "@/types";

// MCP_SERVER_NAME is the name the snippets register Vulcan Code under in other tools.
// MCP_SERVER_NAME 是代码片段在其他工具中注册 Vulcan Code 时使用的名称。
export const MCP_SERVER_NAME = "vulcan-code";

// MCP_CODEX_TOKEN_ENV is the environment variable the Codex snippet reads the token from.
// MCP_CODEX_TOKEN_ENV 是 Codex 代码片段读取令牌的环境变量。
export const MCP_CODEX_TOKEN_ENV = "VULCAN_CODE_MCP_TOKEN";

// MCP_CAPABILITY_LABELS names each grantable media capability with the provider-tool labels.
// MCP_CAPABILITY_LABELS 使用供应商工具标签命名每项可授权的媒体能力。
export const MCP_CAPABILITY_LABELS: Readonly<Record<string, TranslationKey>> = {
  image_generate: "settings.providerTools.imageGenerate",
  image_edit: "settings.providerTools.imageEdit",
  video_generate: "settings.providerTools.videoGenerate",
  video_edit: "settings.providerTools.videoEdit",
  video_extend: "settings.providerTools.videoExtend",
};

// McpSessionPermission names one session flag that depends on read access.
// McpSessionPermission 命名一个依赖读取权限的会话开关。
export type McpSessionPermission = "send" | "create" | "stop" | "answerQuestions";

// MCP_SESSION_PERMISSIONS lists the write flags in display order with their labels.
// MCP_SESSION_PERMISSIONS 按显示顺序列出写权限及其标签。
export const MCP_SESSION_PERMISSIONS: ReadonlyArray<{ key: McpSessionPermission; label: TranslationKey; short: TranslationKey }> = [
  { key: "send", label: "mcpService.grant.send", short: "mcpService.summary.send" },
  { key: "create", label: "mcpService.grant.create", short: "mcpService.summary.create" },
  { key: "stop", label: "mcpService.grant.stop", short: "mcpService.summary.stop" },
  { key: "answerQuestions", label: "mcpService.grant.answer", short: "mcpService.summary.answer" },
];

// emptySessionGrant is the grant of a new client: read-only on every project.
// emptySessionGrant 是新客户端的授权：对所有项目只读。
export function emptySessionGrant(): McpSessionGrantEntry {
  return { read: true, send: false, create: false, stop: false, answerQuestions: false, projects: { mode: "all" } };
}

// withRead turns read access on or off; turning it off also clears every flag that needs it.
// withRead 开启或关闭读取权限；关闭时同时清除所有依赖它的开关。
export function withRead(grant: McpSessionGrantEntry, read: boolean): McpSessionGrantEntry {
  return read ? { ...grant, read } : { ...grant, read, send: false, create: false, stop: false, answerQuestions: false };
}

// withPermission sets one write flag; turning one on also turns read on.
// withPermission 设置一个写权限；开启时同时开启读取权限。
export function withPermission(grant: McpSessionGrantEntry, key: McpSessionPermission, value: boolean): McpSessionGrantEntry {
  return { ...grant, [key]: value, read: value ? true : grant.read };
}

// sameTarget compares two media grants.
// sameTarget 比较两个媒体授权。
export function sameTarget(left: McpMediaTargetEntry, right: McpMediaTargetEntry): boolean {
  return left.connectionId === right.connectionId && left.targetId === right.targetId;
}

// grantSummaryParts lists the short labels of what one client may do.
// grantSummaryParts 列出一个客户端可执行操作的简短标签。
export function grantSummaryParts(
  client: McpServiceClientSummary,
  t: (key: TranslationKey) => string,
  format: (template: string, values: Record<string, string | number>) => string,
): string[] {
  const parts: string[] = [];
  const sessions = client.sessions;
  if (sessions.read) {
    parts.push(t("mcpService.summary.read"));
    for (const permission of MCP_SESSION_PERMISSIONS) {
      if (sessions[permission.key]) parts.push(t(permission.short));
    }
    parts.push(sessions.projects.mode === "all"
      ? t("mcpService.summary.allProjects")
      : format(t("mcpService.summary.projects"), { count: sessions.projects.projectIds.length }));
  } else {
    parts.push(t("mcpService.summary.noSessions"));
  }
  if (client.mediaTargets.length > 0) {
    parts.push(format(t("mcpService.summary.media"), { count: client.mediaTargets.length }));
  }
  return parts;
}

// McpSnippetKind names one connection snippet.
// McpSnippetKind 命名一种连接代码片段。
export type McpSnippetKind = "claude" | "codex" | "json";

// connectionSnippet renders the setup text for one client tool.
// connectionSnippet 为一种客户端工具渲染配置文本。
export function connectionSnippet(kind: McpSnippetKind, endpointUrl: string, token: string): string {
  switch (kind) {
    case "claude":
      return `claude mcp add --transport http ${MCP_SERVER_NAME} ${endpointUrl} --header "Authorization: Bearer ${token}"`;
    case "codex":
      return `[mcp_servers.${MCP_SERVER_NAME}]\nurl = "${endpointUrl}"\nbearer_token_env_var = "${MCP_CODEX_TOKEN_ENV}"`;
    case "json":
      return JSON.stringify({
        mcpServers: {
          [MCP_SERVER_NAME]: { type: "http", url: endpointUrl, headers: { Authorization: `Bearer ${token}` } },
        },
      }, null, 2);
  }
}
