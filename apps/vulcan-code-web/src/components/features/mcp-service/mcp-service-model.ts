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

// Ordered presentation groups keyed by the exact public MCP tool names.
// 使用对外 MCP 工具的精确名称定义展示分组和顺序。
export const MCP_TOOL_GROUPS = [
  { id: "session-read", page: "sessions", label: "mcpService.grant.read", tools: [
    "list_projects", "list_sessions", "get_session", "read_messages", "wait_for_reply", "list_chat_models",
  ] },
  { id: "session-actions", page: "sessions", label: "mcpService.group.sessionActions", tools: [
    "send_message", "create_session", "stop_session", "answer_question",
  ] },
  { id: "files", page: "tools", label: "mcpService.group.files", tools: [
    "file", "file_read", "file_write", "file_edit", "file_move", "file_delete", "apply_patch",
  ] },
  { id: "codekit", page: "tools", label: "mcpService.group.codekit", tools: [
    "vulcan_codekit_repo_map", "vulcan_codekit_ast_tree", "vulcan_codekit_ast_detail", "vulcan_codekit_rg",
    "vulcan_codekit_markdown_menu", "vulcan_codekit_node_source", "vulcan_codekit_patch",
  ] },
  { id: "shell", page: "tools", label: "mcpService.group.shell", tools: ["shell_command"] },
  { id: "knowledge", page: "tools", label: "mcpService.group.knowledge", tools: [
    "knowledge_list", "knowledge_read", "knowledge_search", "knowledge_write",
  ] },
  { id: "media", page: "media", label: "mcpService.group.media", tools: [
    "list_media_models", "generate_media", "get_media_job", "cancel_media_job",
  ] },
] as const satisfies readonly { id: string; page: string; label: TranslationKey; tools: readonly string[] }[];

// A tool page is a tab in the per-client authorization editor.
// 工具页面对应客户端授权编辑器中的一个标签页。
export type McpToolPage = typeof MCP_TOOL_GROUPS[number]["page"];

// Public names that have a dedicated label in the shared catalog.
// 在共享语言目录中有专属标签的公开工具名称。
type McpToolId = typeof MCP_TOOL_GROUPS[number]["tools"][number];

/// Returns the existing session grant description or the tool's own description key.
/// 返回已有会话授权说明或该工具自己的说明键。
export function mcpToolDetailKey(tool: McpToolId): TranslationKey {
  switch (tool) {
    case "send_message": return "mcpService.grant.send";
    case "create_session": return "mcpService.grant.create";
    case "stop_session": return "mcpService.grant.stop";
    case "answer_question": return "mcpService.grant.answer";
    default: return `mcpService.tool.${tool}.detail`;
  }
}

/// Returns whether the selected tool IDs include an operation on a Vulcan Code session.
/// 判断已选工具 ID 中是否存在 Vulcan Code 会话操作。
export function hasMcpSessionTools(selected: readonly string[]): boolean {
  return MCP_TOOL_GROUPS.some((group) => group.page === "sessions" && group.tools.some((id) => selected.includes(id)));
}

/// Updates only the listed tool grants and retains selections from every other group.
/// 只更新指定的工具授权，保留其他分组中的选择。
export function updateMcpToolGrants(selected: string[], tools: readonly string[], checked: boolean): string[] {
  return checked ? [...new Set([...selected, ...tools])] : selected.filter((tool) => !tools.includes(tool));
}

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
  }
  const directToolCount = MCP_TOOL_GROUPS.reduce((count, group) =>
    group.page === "tools" ? count + group.tools.filter((tool) => client.toolIds?.includes(tool)).length : count,
  0);
  if (directToolCount > 0) {
    parts.push(format(t("mcpService.summary.tools"), { count: directToolCount }));
  }
  if (client.mediaTargets.length > 0) {
    parts.push(format(t("mcpService.summary.media"), { count: client.mediaTargets.length }));
  }
  return parts.length > 0 ? parts : [t("mcpService.summary.noSessions")];
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
