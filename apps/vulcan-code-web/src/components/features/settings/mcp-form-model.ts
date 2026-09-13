import type { McpServerSummary } from "@/types"
import type { McpServerSaveRequest } from "./mcp-server-dialog"

// McpLiteral chooses a message in the caller's active application locale.
// McpLiteral 按调用者当前应用语言选择消息。
type McpLiteral = (zh: string, en: string) => string

// chineseLiteral supplies the default for pure validation callers without UI context.
// chineseLiteral 为没有界面上下文的纯校验调用者提供默认语言。
const chineseLiteral: McpLiteral = zh => zh

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
export function validateMcpPairs(rows: Array<{ key: string; value: string }>, label: string, headers = false, text: McpLiteral = chineseLiteral): string | null {
  // names tracks canonical HTTP header identity or exact environment names.
  // names 跟踪规范 HTTP 请求头标识或精确环境变量名。
  const names = new Set<string>()
  for (const [index, row] of rows.entries()) {
    // key and identity implement the transport-specific duplicate rule.
    // key 与 identity 实现对应传输类型的重复规则。
    const key = row.key.trim()
    const identity = headers ? key.toLowerCase() : key
    if (!key || !row.value.trim()) return `${label} ${text(`第 ${index + 1} 行：键和值必填`, `Row ${index + 1}: key and value required`)}`
    if (names.has(identity)) return `${label} ${text(`第 ${index + 1} 行：名称重复`, `Row ${index + 1}: duplicate name`)}`
    if (headers ? !/^[!#$%&'*+.^_\x60|~0-9A-Za-z-]+$/.test(key) : /[=\0]/.test(key)) return `${label} ${text(`第 ${index + 1} 行：名称无效`, `Row ${index + 1}: invalid name`)}`
    if (/[\r\n\0]/.test(row.value)) return `${label} ${text(`第 ${index + 1} 行：值含无效控制字符`, `Row ${index + 1}: invalid control character`)}`
    names.add(identity)
  }
  return null
}

// validateMcpDraft validates only the selected transport without splitting argument strings.
// validateMcpDraft 仅校验所选传输类型，不拆分参数字符串。
export function validateMcpDraft(draft: McpServerSaveRequest, text: McpLiteral = chineseLiteral): string | null {
  if (!draft.name.trim()) return text("请填写名称", "Name is required")
  if (!["stdio", "http", "sse"].includes(draft.transport)) return text("传输类型无效", "Invalid transport")
  if (draft.transport === "stdio") {
    if (!draft.command.trim()) return text("请填写可执行命令", "Executable is required")
    if ([draft.command, draft.cwd, ...draft.args].some(value => value.includes("\0"))) return text("命令、目录与参数不能包含 NUL", "NUL is not allowed")
    if (draft.envPassthrough.some(name => !name.trim() || /[=\0\r\n]/.test(name))) return text("透传变量名无效", "Invalid passthrough variable name")
    if (new Set(draft.envPassthrough.map(name => name.trim())).size !== draft.envPassthrough.length) return text("透传变量名重复", "Duplicate passthrough variable")
    return validateMcpPairs(draft.env, "Env", false, text)
  }
  try {
    // endpoint is parsed strictly before the backend receives the request.
    // endpoint 在后端接收请求前经过严格解析。
    const endpoint = new URL(draft.url)
    if (!["http:", "https:"].includes(endpoint.protocol) || !endpoint.hostname) return text("需要 HTTP(S) 地址", "HTTP(S) URL required")
  } catch { return text("端点 URL 无效", "Invalid endpoint URL") }
  if (draft.headersFromEnv.some(row => /[=\0\r\n]/.test(row.value))) return text("请求头映射的宿主变量名无效", "Invalid host variable name for a header")
  // Exact same-name mappings preserve the runtime's explicit-header precedence; case variants are ambiguous in HTTP.
  // 精确同名映射保持运行时静态头优先规则；仅大小写不同的 HTTP 头存在歧义。
  if (draft.headersFromEnv.some(mapped => draft.headers.some(explicit => explicit.key.trim().toLowerCase() === mapped.key.trim().toLowerCase() && explicit.key.trim() !== mapped.key.trim()))) {
    return text("静态头与环境映射的同名键须使用一致大小写", "Use identical casing for a header shared by static and environment mappings")
  }
  if (draft.oauthScopes.some(scope => !scope.trim())) return text("OAuth scope 不能留空", "OAuth scopes must not be empty")
  return validateMcpPairs(draft.headers, "Headers", true, text) ?? validateMcpPairs(draft.headersFromEnv, "Headers from env", true, text)
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
