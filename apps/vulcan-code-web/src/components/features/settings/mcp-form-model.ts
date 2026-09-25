import type { McpServerSummary } from "@/types"
import type { McpServerSaveRequest } from "./mcp-server-dialog"
import { formatMessage, type Translate } from "@/i18n"

// createMcpDraft copies persisted request fields without sharing mutable arrays.
// createMcpDraft 复制持久化请求字段，不共享可变数组。
export function createMcpDraft(server: McpServerSummary | null): McpServerSaveRequest {
  return {
    serverId: server?.id ?? null, name: server?.name ?? "", enabled: server?.enabled ?? true,
    transport: server?.transport ?? "stdio", command: server?.command ?? "", args: [...(server?.args ?? [])],
    url: server?.url ?? "", bearerToken: server?.bearerToken ?? "", headers: (server?.headers ?? []).map(row => ({ ...row })),
    headersFromEnv: (server?.headersFromEnv ?? []).map(row => ({ ...row })), oauthClientId: server?.oauthClientId ?? "",
    oauthClientSecret: server?.oauthClientSecret ?? "", oauthScopes: [...(server?.oauthScopes ?? [])],
    oauthRedirectUri: server?.oauthRedirectUri ?? "", cwd: server?.cwd ?? "", env: (server?.env ?? []).map(row => ({ ...row })),
    envPassthrough: [...(server?.envPassthrough ?? [])],
  }
}

// validateMcpPairs returns row-specific errors without including secret values.
// validateMcpPairs 返回逐行错误，不包含密钥值。
export function validateMcpPairs(rows: Array<{ key: string; value: string }>, label: string, headers = false, t: Translate): string | null {
  // names tracks canonical HTTP header identity or exact environment names.
  // names 跟踪规范 HTTP 请求头标识或精确环境变量名。
  const names = new Set<string>()
  for (const [index, row] of rows.entries()) {
    // key and identity implement the transport-specific duplicate rule.
    // key 与 identity 实现对应传输类型的重复规则。
    const key = row.key.trim()
    const identity = headers ? key.toLowerCase() : key
    if (!key || !row.value.trim()) return `${label} ${formatMessage(t("settings.mcp.rowKeyAndValue"), { index: index + 1 })}`
    if (names.has(identity)) return `${label} ${formatMessage(t("settings.mcp.rowDuplicateName"), { index: index + 1 })}`
    if (headers ? !/^[!#$%&'*+.^_\x60|~0-9A-Za-z-]+$/.test(key) : /[=\0]/.test(key)) return `${label} ${formatMessage(t("settings.mcp.rowInvalidName"), { index: index + 1 })}`
    if (/[\r\n\0]/.test(row.value)) return `${label} ${formatMessage(t("settings.mcp.rowInvalidControlCharacter"), { index: index + 1 })}`
    names.add(identity)
  }
  return null
}

// validateMcpDraft validates only the selected transport without splitting argument strings.
// validateMcpDraft 仅校验所选传输类型，不拆分参数字符串。
export function validateMcpDraft(draft: McpServerSaveRequest, t: Translate): string | null {
  if (!draft.name.trim()) return t("settings.mcp.nameIsRequired")
  if (!["stdio", "http", "sse"].includes(draft.transport)) return t("settings.mcp.invalidTransport")
  if (draft.transport === "stdio") {
    if (!draft.command.trim()) return t("settings.mcp.executableIsRequired")
    if ([draft.command, draft.cwd, ...draft.args].some(value => value.includes("\0"))) return t("settings.mcp.nulIsNotAllowed")
    if (draft.envPassthrough.some(name => !name.trim() || /[=\0\r\n]/.test(name))) return t("settings.mcp.invalidPassthroughVariableName")
    if (new Set(draft.envPassthrough.map(name => name.trim())).size !== draft.envPassthrough.length) return t("settings.mcp.duplicatePassthroughVariable")
    return validateMcpPairs(draft.env, "Env", false, t)
  }
  try {
    // endpoint is parsed strictly before the backend receives the request.
    // endpoint 在后端接收请求前经过严格解析。
    const endpoint = new URL(draft.url)
    if (!["http:", "https:"].includes(endpoint.protocol) || !endpoint.hostname) return t("settings.mcp.httpSUrlRequired")
  } catch { return t("settings.mcp.invalidEndpointUrl") }
  if (draft.headersFromEnv.some(row => /[=\0\r\n]/.test(row.value))) return t("settings.mcp.invalidHostVariableName")
  // Exact same-name mappings preserve the runtime's explicit-header precedence; case variants are ambiguous in HTTP.
  // 精确同名映射保持运行时静态头优先规则；仅大小写不同的 HTTP 头存在歧义。
  if (draft.headersFromEnv.some(mapped => draft.headers.some(explicit => explicit.key.trim().toLowerCase() === mapped.key.trim().toLowerCase() && explicit.key.trim() !== mapped.key.trim()))) {
    return t("settings.mcp.useIdenticalCasingFor")
  }
  if (draft.oauthScopes.some(scope => !scope.trim())) return t("settings.mcp.oauthScopesMustNot")
  return validateMcpPairs(draft.headers, "Headers", true, t) ?? validateMcpPairs(draft.headersFromEnv, "Headers from env", true, t)
}

// parseMcpBulkPairs splits at the first equals sign and preserves all later value characters.
// parseMcpBulkPairs 仅在第一个等号拆分，并保留其后的全部值字符。
export function parseMcpBulkPairs(text: string): Array<{ key: string; value: string }> {
  return text.split(/\r?\n/).filter(line => line.trim()).map(line => {
    // separator distinguishes malformed input from values containing equals signs.
    // separator 区分格式错误输入与包含等号的值。
    const separator = line.indexOf("=")
    return { key: separator < 0 ? line : line.slice(0, separator), value: separator < 0 ? "" : line.slice(separator + 1) }
  })
}
