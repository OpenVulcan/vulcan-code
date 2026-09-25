import { Button } from "@/components/ui/button"
import { DisclosureSection } from "@/components/ui/disclosure"
import { createMcpDraft, validateMcpDraft } from "./mcp-form-model"
import { McpStringRows, McpKeyValueRows } from "./mcp-form-fields"
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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import type { McpServerSummary } from "@/types"
import { useI18n } from "@/i18n"

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

// McpServerDialog owns a draft for one open lifetime and retains it after service failures.
// McpServerDialog 在单次打开期间拥有草稿，并在服务失败后保留它。
export function McpServerDialog(props: {
  actions: McpSettingsActionProps; open: boolean; onOpenChange: (open: boolean) => void; server: McpServerSummary | null
}) {
  const { t } = useI18n();
  // text follows the shared application locale.
  // text 遵循共享应用语言。

  // draft and operation state belong to this modal, not the changing workbench snapshot.
  // draft 与操作状态属于此弹窗，不随工作台快照变化。
  const [draft, setDraft] = React.useState<McpServerSaveRequest>(() => createMcpDraft(null))
  const [busy, setBusy] = React.useState(false)
  const [error, setError] = React.useState("")
  const saving = React.useRef(false)
  const openLifetime = React.useRef(false)
  React.useEffect(() => {
    if (props.open && !openLifetime.current) { setDraft(createMcpDraft(props.server)); setError("") }
    openLifetime.current = props.open
  }, [props.open, props.server])
  // change updates exactly one declared request field.
  // change 仅更新一个已声明的请求字段。
  const change = <K extends keyof McpServerSaveRequest>(key: K, value: McpServerSaveRequest[K]) => setDraft(current => ({ ...current, [key]: value }))
  // save validates before dispatch and prevents duplicate submissions.
  // save 在派发前校验，并阻止重复提交。
  const save = async () => {
    if (saving.current) return
    const issue = validateMcpDraft(draft, t)
    if (issue) { setError(issue); return }
    saving.current = true; setBusy(true); setError("")
    try { await props.actions.onSaveMcpServer(draft); props.onOpenChange(false) }
    catch (failure) { setError(String(failure)) }
    finally { saving.current = false; setBusy(false) }
  }
  return <Dialog open={props.open} onOpenChange={open => { if (!saving.current) props.onOpenChange(open) }}>
    <DialogContent className="flex min-h-0 flex-col" scrollMode="fixed-footer" size="xl">
      <DialogHeader className="shrink-0"><DialogTitle>{draft.serverId ? t("settings.mcp.editMcpServer") : t("settings.mcp.addMcpServer")}</DialogTitle>
        <DialogDescription>{t("settings.mcp.itConnectsAutomaticallyAfter")}</DialogDescription>
      </DialogHeader>
      <div className="mcp-dialog-body">
      <fieldset disabled={busy} className="mcp-form">
        <div className="settings-form-grid two">
          <label className="settings-form-field"><span>{t("settings.mcp.name")}</span><Input value={draft.name} onChange={e => change("name", e.target.value)} /></label>
          <div className="settings-form-field"><span>{t("settings.mcp.transport")}</span><Select value={draft.transport} disabled={busy} onValueChange={value => change("transport", value)}>
            <SelectTrigger aria-label={t("settings.mcp.transport")}><SelectValue /></SelectTrigger><SelectContent><SelectItem value="stdio">{t("settings.mcp.stdio")}</SelectItem><SelectItem value="http">Streamable HTTP</SelectItem><SelectItem value="sse">SSE</SelectItem></SelectContent>
          </Select></div>
          <label className="settings-check-row inline"><Switch checked={draft.enabled} disabled={busy} onCheckedChange={value => change("enabled", value)} />{t("settings.mcp.enabled")}</label>
        </div>
        {draft.transport === "stdio" ? <>
          <label className="settings-form-field"><span>{t("settings.mcp.executable")}</span><Input placeholder="node" value={draft.command} onChange={e => change("command", e.target.value)} /></label>
          <p className="settings-muted-line">{t("settings.mcp.enterOnlyTheProgram")}</p>
          <McpStringRows label={t("settings.mcp.arguments")} values={draft.args} onChange={value => change("args", value)} placeholder="server.js" exact />
          <label className="settings-form-field"><span>{t("settings.mcp.workingDirectory")}</span><Input value={draft.cwd} onChange={e => change("cwd", e.target.value)} /></label>
          <p className="settings-muted-line">{t("settings.mcp.leaveEmptyToUse")}</p>
          <McpKeyValueRows label={t("settings.mcp.environmentVariables")} values={draft.env} onChange={value => change("env", value)} secret />
          <McpStringRows label={t("settings.mcp.variablesPassedFromThis")} values={draft.envPassthrough} onChange={value => change("envPassthrough", value)} placeholder="API_TOKEN" />
          <p className="settings-muted-line">{t("settings.mcp.theProcessAlreadySees")}</p>
        </> : <>
          <label className="settings-form-field"><span>{t("settings.mcp.serverUrl")}</span><Input placeholder="https://example.com/mcp" value={draft.url} onChange={e => change("url", e.target.value)} /></label>
          <section className="mcp-form-section"><h3>{t("settings.mcp.headersAndAuthentication")}</h3>
            <label className="settings-form-field"><span>Bearer Token</span><Input autoComplete="off" type="password" value={draft.bearerToken} onChange={e => change("bearerToken", e.target.value)} /></label>
            <McpKeyValueRows headers label={t("settings.mcp.fixedHeaders")} values={draft.headers} onChange={value => change("headers", value)} secret />
            <McpKeyValueRows headers label={t("settings.mcp.headersReadFromEnvironment")} values={draft.headersFromEnv} onChange={value => change("headersFromEnv", value)} valuePlaceholder="ACCESS_TOKEN" />
            <p className="settings-muted-line">{t("settings.mcp.headerNameOnThe")}</p>
          </section>
          <DisclosureSection className="mcp-oauth" description={t("settings.mcp.onlyNeededIfThe")} title={t("settings.mcp.oauthSignIn")}>
            <div className="mcp-form-section">
            <label className="settings-form-field"><span>Client ID</span><Input value={draft.oauthClientId} onChange={e => change("oauthClientId", e.target.value)} /></label>
            <label className="settings-form-field"><span>Client secret</span><Input autoComplete="off" type="password" value={draft.oauthClientSecret} onChange={e => change("oauthClientSecret", e.target.value)} /></label>
            <McpStringRows label="OAuth scopes" values={draft.oauthScopes} onChange={value => change("oauthScopes", value)} placeholder="read" />
            <label className="settings-form-field"><span>Redirect URI</span><Input value={draft.oauthRedirectUri} onChange={e => change("oauthRedirectUri", e.target.value)} /></label>
            </div>
          </DisclosureSection>
        </>}
        {error && <p role="alert" className="mcp-error">{error}</p>}
      </fieldset>
      </div>
      <DialogFooter className="shrink-0"><Button variant="outline" disabled={busy} onClick={() => props.onOpenChange(false)}>{t("settings.mcp.cancel")}</Button><Button disabled={busy} onClick={() => void save()}>{busy ? t("settings.mcp.saving") : t("settings.mcp.save")}</Button></DialogFooter>
    </DialogContent>
  </Dialog>
}
