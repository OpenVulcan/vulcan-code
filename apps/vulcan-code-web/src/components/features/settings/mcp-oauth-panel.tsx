import * as React from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { cancelMcpOAuth, finishMcpOAuth, loadMcpOAuthStatus, removeMcpOAuth, startMcpOAuth, type McpOAuthStatus } from "@/service/mcp-settings-service"
import { openExternalUrl } from "@/service/project-actions-service"
import { formatMessage, useI18n, type Translate } from "@/i18n"

// Poll interval while a sign-in flow waits for the browser or exchanges the code.
// 登录流程等待浏览器或交换授权码期间的轮询间隔。
const ACTIVE_FLOW_POLL_MS = 2000

// authStatusText describes the stored credential state, or why it is unknown.
// authStatusText 描述已存储凭据的状态，或其未知的原因。
function authStatusText(status: McpOAuthStatus, t: Translate): string {
  switch (status.authStatus) {
    case "authenticated": return t("settings.mcp.oauth.statusAuthenticated")
    case "expired": return t("settings.mcp.oauth.statusExpired")
    case "not_authenticated": return t("settings.mcp.oauth.statusNotAuthenticated")
    case "not_configured": return t("settings.mcp.oauth.statusNotConfigured")
    default: return formatMessage(t("settings.mcp.oauth.statusUnknown"), { error: status.statusError ?? "" })
  }
}

// McpOAuthPanel signs one server in or out; tokens never reach this page, only status codes.
// McpOAuthPanel 为单个服务器登录或退出；令牌从不到达本页面，只有状态代码。
export function McpOAuthPanel(props: {
  // Exact server identity used by every OAuth operation.
  // 所有 OAuth 操作使用的精确服务器标识。
  serverId: string
  // Runtime connection state; a change re-reads the credential status.
  // 运行时连接状态；变化时重新读取凭据状态。
  connectionStatus: string | undefined
}) {
  const { t } = useI18n()
  // status and error belong to the selected server and reset when it changes.
  // status 与 error 属于所选服务器，切换时重置。
  const [status, setStatus] = React.useState<McpOAuthStatus | null>(null)
  const [error, setError] = React.useState("")
  // busyServers names every server with an operation in flight, so starting one on another server
  // never re-enables this one while its own request is still pending.
  // busyServers 记录所有操作进行中的服务器，因此在其他服务器上开始操作不会让本服务器在其请求仍未完成时
  // 重新可用。
  const [busyServers, setBusyServers] = React.useState<ReadonlySet<string>>(() => new Set())
  const busy = busyServers.has(props.serverId)
  const [callbackUrl, setCallbackUrl] = React.useState("")
  // serverRef discards responses that arrive after the selection moved to another server.
  // serverRef 丢弃选择切换到其他服务器后才到达的响应。
  const serverRef = React.useRef(props.serverId)
  serverRef.current = props.serverId
  const accept = React.useCallback((serverId: string, next: McpOAuthStatus) => {
    if (serverRef.current === serverId) setStatus(next)
  }, [])
  React.useEffect(() => { setStatus(null); setError(""); setCallbackUrl("") }, [props.serverId])
  // phase drives polling: only an unfinished flow changes without a user action.
  // phase 决定轮询：只有未结束的流程会在无用户操作时变化。
  const phase = status?.flow?.phase
  const flowActive = phase === "awaiting_callback" || phase === "exchanging"
  React.useEffect(() => {
    let active = true
    const serverId = props.serverId
    const refresh = () => loadMcpOAuthStatus(serverId)
      .then(next => { if (active) accept(serverId, next) })
      .catch(failure => { if (active) setError(String(failure)) })
    void refresh()
    const timer = flowActive ? window.setInterval(() => void refresh(), ACTIVE_FLOW_POLL_MS) : undefined
    return () => { active = false; if (timer !== undefined) window.clearInterval(timer) }
  }, [props.serverId, props.connectionStatus, flowActive, accept])
  // run serializes one explicit operation and keeps its error until the next one.
  // run 串行执行一次显式操作，并保留其错误直到下一次操作。
  const run = async (operation: () => Promise<McpOAuthStatus>) => {
    if (busy) return
    const serverId = props.serverId
    setBusyServers(current => new Set(current).add(serverId)); setError("")
    try { accept(serverId, await operation()) }
    catch (failure) { if (serverRef.current === serverId) setError(String(failure)) }
    finally {
      setBusyServers(current => {
        const next = new Set(current)
        next.delete(serverId)
        return next
      })
    }
  }
  const flow = status?.flow ?? null
  const signedIn = status?.authStatus === "authenticated" || status?.authStatus === "expired"
  // Sign-out also clears credentials whose status is unknown, for example while the server is stopped.
  // 退出登录也会清除状态未知的凭据，例如服务器已停止时。
  const canSignOut = signedIn || (status !== null && status.authStatus === null)
  return <section className="mcp-form-section mcp-oauth-panel" aria-label={t("settings.mcp.oauthSignIn")}>
    <h4>{t("settings.mcp.oauthSignIn")}</h4>
    <p role="status">{status ? authStatusText(status, t) : t("settings.mcp.loading")}</p>
    {flow && flowActive && <>
      <p className="settings-muted-line">{phase === "exchanging" ? t("settings.mcp.oauth.exchanging") : t("settings.mcp.oauth.waitingForBrowser")}</p>
      <p className="settings-muted-line"><code className="break-all">{flow.authorizationUrl}</code></p>
      {phase === "awaiting_callback" && flow.callbackMode === "manual" && <>
        <label className="settings-form-field"><span>{t("settings.mcp.oauth.callbackUrl")}</span>
          <Input autoComplete="off" value={callbackUrl} disabled={busy} placeholder={flow.redirectUri} onChange={e => setCallbackUrl(e.target.value)} />
        </label>
        <p className="settings-muted-line">{t("settings.mcp.oauth.manualHint")}</p>
      </>}
      <div className="mcp-filter-row">
        {phase === "awaiting_callback" && flow.callbackMode === "manual" && <Button size="sm" disabled={busy || !callbackUrl.trim()} onClick={() => void run(() => finishMcpOAuth({ serverId: props.serverId, flowId: flow.flowId, callbackUrl }))}>{t("settings.mcp.oauth.finish")}</Button>}
        {/* The click is the explicit confirmation for this exact authorization URL. */}
        {/* 该点击即为对此精确授权 URL 的显式确认。 */}
        <Button size="sm" variant="outline" disabled={busy || phase !== "awaiting_callback"} onClick={() => void openExternalUrl(flow.authorizationUrl, true).catch(failure => setError(String(failure)))}>{t("settings.mcp.oauth.openPage")}</Button>
        <Button size="sm" variant="ghost" disabled={busy || phase !== "awaiting_callback"} onClick={() => void run(() => cancelMcpOAuth({ serverId: props.serverId, flowId: flow.flowId }))}>{t("settings.mcp.oauth.cancel")}</Button>
      </div>
    </>}
    {flow?.phase === "failed" && <p role="alert" className="mcp-error">{formatMessage(t("settings.mcp.oauth.failed"), { error: flow.error ?? "" })}</p>}
    {flow?.phase === "expired" && <p role="alert" className="mcp-error">{t("settings.mcp.oauth.expired")}</p>}
    {!flowActive && status?.authStatus !== "not_configured" && <div className="mcp-filter-row">
      <Button size="sm" disabled={busy || !status} onClick={() => { setCallbackUrl(""); void run(() => startMcpOAuth(props.serverId)) }}>{signedIn ? t("settings.mcp.oauth.signInAgain") : t("settings.mcp.oauth.signIn")}</Button>
      {canSignOut && <Button size="sm" variant="outline" disabled={busy} onClick={() => void run(() => removeMcpOAuth(props.serverId))}>{t("settings.mcp.oauth.signOut")}</Button>}
    </div>}
    {error && <p role="alert" className="mcp-error">{error}</p>}
  </section>
}
