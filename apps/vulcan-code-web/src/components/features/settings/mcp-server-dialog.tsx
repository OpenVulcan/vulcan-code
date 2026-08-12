import * as React from "react"

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { useI18n } from "@/i18n"
import type { McpServerSummary } from "@/types"

// McpServerSaveRequest describes one MCP server configuration save request.
// McpServerSaveRequest 描述单次 MCP 服务器配置保存请求。
export interface McpServerSaveRequest {
  // Existing server id when editing, or null when creating a new server.
  // 编辑现有服务器时为服务器标识，新建服务器时为空。
  serverId: string | null
  // User-facing server name.
  // 面向用户展示的服务器名称。
  name: string
  // Whether the server is enabled.
  // 服务器是否启用。
  enabled: boolean
  // Transport kind used by the MCP server.
  // MCP 服务器使用的传输类型。
  transport: string
  // Stdio command.
  // Stdio 命令。
  command: string
  // Stdio command arguments.
  // Stdio 命令参数。
  args: string[]
  // HTTP or SSE endpoint URL.
  // HTTP 或 SSE 端点地址。
  url: string
  // Optional bearer token.
  // 可选 Bearer Token。
  bearerToken: string
  // Static request headers.
  // 静态请求头。
  headers: Array<{ key: string; value: string }>
  // Request headers sourced from environment variables.
  // 从环境变量读取的请求头。
  headersFromEnv: Array<{ key: string; value: string }>
  // OAuth client id.
  // OAuth 客户端标识。
  oauthClientId: string
  // OAuth client secret.
  // OAuth 客户端密钥。
  oauthClientSecret: string
  // OAuth scopes.
  // OAuth scope 列表。
  oauthScopes: string[]
  // OAuth redirect URI.
  // OAuth 重定向 URI。
  oauthRedirectUri: string
  // Working directory for stdio servers.
  // stdio 服务器工作目录。
  cwd: string
  // Environment variables.
  // 环境变量。
  env: Array<{ key: string; value: string }>
  // Environment variable names passed through from the host.
  // 从宿主透传的环境变量名称。
  envPassthrough: string[]
}

// McpSettingsActionProps narrows settings actions to the MCP server operations used here.
// McpSettingsActionProps 将设置动作收窄为此处使用的 MCP 服务器操作。
export interface McpSettingsActionProps {
  // Saves one MCP server configuration.
  // 保存单个 MCP 服务器配置。
  onSaveMcpServer: (request: McpServerSaveRequest) => Promise<void>
  // Removes one MCP server configuration by id.
  // 按标识移除单个 MCP 服务器配置。
  onRemoveMcpServer: (serverId: string) => Promise<void>
}

// McpServerDialog renders the MCP server add/edit form.
// McpServerDialog 渲染 MCP 服务器新增/编辑表单。
export function McpServerDialog(props: {
  // Service-backed MCP settings actions.
  // 服务端支持的 MCP 设置动作。
  actions: McpSettingsActionProps
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Server being edited.
  // 当前编辑的服务器。
  server: McpServerSummary | null
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [name, setName] = React.useState("")
  const [enabled, setEnabled] = React.useState(true)
  const [transport, setTransport] = React.useState("stdio")
  const [command, setCommand] = React.useState("")
  const [argsText, setArgsText] = React.useState("")
  const [url, setUrl] = React.useState("")
  const [bearerToken, setBearerToken] = React.useState("")
  const [headersText, setHeadersText] = React.useState("")
  const [headersFromEnvText, setHeadersFromEnvText] = React.useState("")
  const [oauthClientId, setOauthClientId] = React.useState("")
  const [oauthClientSecret, setOauthClientSecret] = React.useState("")
  const [oauthScopesText, setOauthScopesText] = React.useState("")
  const [oauthRedirectUri, setOauthRedirectUri] = React.useState("")
  const [cwd, setCwd] = React.useState("")
  const [envText, setEnvText] = React.useState("")
  const [envPassthroughText, setEnvPassthroughText] = React.useState("")
  React.useEffect(() => {
    const server = props.server
    setName(server?.name ?? "")
    setEnabled(server?.enabled ?? true)
    setTransport(server?.transport ?? "stdio")
    setCommand(server?.command ?? "")
    setArgsText((server?.args ?? []).join("\n"))
    setUrl(server?.url ?? "")
    setBearerToken(server?.bearerToken ?? "")
    setHeadersText(formatKeyValueLines(server?.headers ?? []))
    setHeadersFromEnvText(formatKeyValueLines(server?.headersFromEnv ?? []))
    setOauthClientId(server?.oauthClientId ?? "")
    setOauthClientSecret(server?.oauthClientSecret ?? "")
    setOauthScopesText((server?.oauthScopes ?? []).join("\n"))
    setOauthRedirectUri(server?.oauthRedirectUri ?? "")
    setCwd(server?.cwd ?? "")
    setEnvText(formatKeyValueLines(server?.env ?? []))
    setEnvPassthroughText((server?.envPassthrough ?? []).join("\n"))
  }, [props.server])
  const save = React.useCallback(async () => {
    await props.actions.onSaveMcpServer({
      serverId: props.server?.id ?? null,
      name,
      enabled,
      transport,
      command,
      args: parseLines(argsText),
      url,
      bearerToken,
      headers: parseKeyValueLines(headersText),
      headersFromEnv: parseKeyValueLines(headersFromEnvText),
      oauthClientId,
      oauthClientSecret,
      oauthScopes: parseLines(oauthScopesText),
      oauthRedirectUri,
      cwd,
      env: parseKeyValueLines(envText),
      envPassthrough: parseLines(envPassthroughText),
    })
    props.onOpenChange(false)
  }, [argsText, bearerToken, command, cwd, enabled, envPassthroughText, envText, headersFromEnvText, headersText, name, oauthClientId, oauthClientSecret, oauthRedirectUri, oauthScopesText, props, transport, url])
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog wide">
        <DialogHeader>
          <DialogTitle>{props.server ? text("编辑 MCP 服务器", "Edit MCP server") : text("添加 MCP 服务器", "Add MCP server")}</DialogTitle>
          <DialogDescription>{text("多值字段每行一个；键值字段使用 KEY=VALUE。", "Use one line per value; key/value fields use KEY=VALUE.")}</DialogDescription>
        </DialogHeader>
        <div className="settings-form-grid two">
          <label className="settings-form-field">
            <span>{text("名称", "Name")}</span>
            <Input onChange={(event) => setName(event.target.value)} value={name} />
          </label>
          <label className="settings-form-field">
            <span>{text("传输", "Transport")}</span>
            <select className="settings-input" onChange={(event) => setTransport(event.target.value)} value={transport}>
              <option value="stdio">stdio</option>
              <option value="http">http</option>
              <option value="sse">sse</option>
            </select>
          </label>
          <label className="settings-check-row inline">
            <input checked={enabled} onChange={(event) => setEnabled(event.target.checked)} type="checkbox" />
            <span>{text("启用服务器", "Enable server")}</span>
          </label>
          {transport === "stdio" ? (
            <>
              <label className="settings-form-field">
                <span>{text("命令", "Command")}</span>
                <Input onChange={(event) => setCommand(event.target.value)} value={command} />
              </label>
              <label className="settings-form-field">
                <span>{text("参数", "Arguments")}</span>
                <Textarea onChange={(event) => setArgsText(event.target.value)} value={argsText} />
              </label>
              <label className="settings-form-field">
                <span>CWD</span>
                <Input onChange={(event) => setCwd(event.target.value)} value={cwd} />
              </label>
              <label className="settings-form-field">
                <span>Env</span>
                <Textarea onChange={(event) => setEnvText(event.target.value)} value={envText} />
              </label>
              <label className="settings-form-field">
                <span>{text("环境透传", "Env passthrough")}</span>
                <Textarea onChange={(event) => setEnvPassthroughText(event.target.value)} value={envPassthroughText} />
              </label>
            </>
          ) : (
            <>
              <label className="settings-form-field">
                <span>URL</span>
                <Input onChange={(event) => setUrl(event.target.value)} value={url} />
              </label>
              <label className="settings-form-field">
                <span>Bearer Token</span>
                <Input onChange={(event) => setBearerToken(event.target.value)} type="password" value={bearerToken} />
              </label>
              <label className="settings-form-field">
                <span>Headers</span>
                <Textarea onChange={(event) => setHeadersText(event.target.value)} value={headersText} />
              </label>
              <label className="settings-form-field">
                <span>Headers from env</span>
                <Textarea onChange={(event) => setHeadersFromEnvText(event.target.value)} value={headersFromEnvText} />
              </label>
              <label className="settings-form-field">
                <span>OAuth Client ID</span>
                <Input onChange={(event) => setOauthClientId(event.target.value)} value={oauthClientId} />
              </label>
              <label className="settings-form-field">
                <span>OAuth Client Secret</span>
                <Input onChange={(event) => setOauthClientSecret(event.target.value)} type="password" value={oauthClientSecret} />
              </label>
              <label className="settings-form-field">
                <span>OAuth Scopes</span>
                <Textarea onChange={(event) => setOauthScopesText(event.target.value)} value={oauthScopesText} />
              </label>
              <label className="settings-form-field">
                <span>OAuth Redirect URI</span>
                <Input onChange={(event) => setOauthRedirectUri(event.target.value)} value={oauthRedirectUri} />
              </label>
            </>
          )}
        </div>
        <DialogFooter>
          <button className="skill-action-button secondary" onClick={() => props.onOpenChange(false)} type="button">{text("取消", "Cancel")}</button>
          <button className="skill-action-button" onClick={() => void save()} type="button">{text("保存", "Save")}</button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// localizedLiteral picks the display literal that matches the active locale.
// localizedLiteral 按当前语言选择展示文本。
function localizedLiteral(locale: string, zh: string, en: string): string {
  return locale === "zh-CN" ? zh : en
}

// parseLines converts a multiline text area into a trimmed string list.
// parseLines 将多行文本框转换为裁剪后的字符串列表。
function parseLines(value: string): string[] {
  return value.split(/\r?\n/g).map((line) => line.trim()).filter(Boolean)
}

// parseKeyValueLines converts KEY=VALUE lines into key/value rows.
// parseKeyValueLines 将 KEY=VALUE 行转换为键值行。
function parseKeyValueLines(value: string): Array<{ key: string; value: string }> {
  return parseLines(value).map((line) => {
    const separatorIndex = line.indexOf("=")
    if (separatorIndex < 0) {
      return { key: line, value: "" }
    }
    return {
      key: line.slice(0, separatorIndex).trim(),
      value: line.slice(separatorIndex + 1).trim(),
    }
  })
}

// formatKeyValueLines converts key/value rows into KEY=VALUE lines.
// formatKeyValueLines 将键值行转换为 KEY=VALUE 行。
function formatKeyValueLines(values: Array<{ key: string; value: string }>): string {
  return values.map((entry) => `${entry.key}=${entry.value}`).join("\n")
}
