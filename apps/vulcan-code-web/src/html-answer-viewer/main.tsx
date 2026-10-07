import { createRoot } from "react-dom/client";
import { I18nProvider } from "@/i18n";
import { ensureLocaleCatalog } from "@/i18n-catalogs";
import { normalizeLocaleCode, resolveAppLocale, type LocaleCode } from "@/i18n-locales";
import { exchangeHtmlAnswerTicket, HtmlAnswerViewerError, parseHtmlAnswerTicket, releaseHtmlAnswerLease } from "./transport";
import { HtmlAnswerViewer, type HtmlAnswerViewerState } from "./viewer";
import "./viewer.css";

// Capture then immediately remove the bearer fragment, before any asynchronous operation or render.
// 捕获后立即移除持有者片段，早于任何异步操作或渲染。
const entryUrl = new URL(window.location.href);
window.history.replaceState(null, "", window.location.pathname);
// The viewer never imports service discovery, general RPC, desktop preload or workbench persistence.
// 查看器绝不导入服务发现、通用 RPC、桌面预加载或工作台持久化模块。
const root = createRoot(document.getElementById("root")!);
const abort = new AbortController();
let disposed = false;
let activeLease: string | null = null;
let expiry: ReturnType<typeof setTimeout> | undefined;
let locale: LocaleCode = resolveAppLocale(null);

/** Render trusted state only while the owning document remains active.
 * 仅在所属文档仍活跃时渲染可信状态。 */
function render(state: HtmlAnswerViewerState): void {
  if (!disposed) root.render(<I18nProvider initialLocale={locale}><HtmlAnswerViewer state={state} /></I18nProvider>);
}

/** Revoke the current lease and clear its timer without throwing during document teardown.
 * 撤销当前租约并清理定时器，文档拆卸期间不抛出错误。 */
function release(): void {
  clearTimeout(expiry);
  if (activeLease !== null) { releaseHtmlAnswerLease(activeLease); activeLease = null; }
}

/** Dispose execution on pagehide; a browser-history restore must obtain a fresh link.
 * 在 pagehide 时释放执行；浏览器历史恢复必须取得新链接。 */
function dispose(): void { disposed = true; abort.abort(); release(); root.unmount(); }
window.addEventListener("pagehide", dispose, { once: true });
// Restored documents cannot reuse a released lease or silently exchange a consumed fragment.
// 恢复的文档不能复用已释放租约，也不能静默兑换已消费片段。
window.addEventListener("pageshow", (event) => { if (event.persisted) window.location.reload(); });
render({ status: "loading" });

/** Perform one exchange outside React effects so StrictMode or re-renders cannot consume the ticket twice.
 * 在 React effect 之外完成一次兑换，避免 StrictMode 或重渲染重复消费票据。 */
async function load(): Promise<void> {
  try {
    const ticket = parseHtmlAnswerTicket(entryUrl);
    // Remove the retained fragment as well; only the bounded exchange request now owns its bytes.
    // 同时移除保留 URL 中的片段；之后仅有界兑换请求持有其字节。
    entryUrl.hash = "";
    const lease = await exchangeHtmlAnswerTicket(ticket, abort.signal);
    const receivedAt = performance.now();
    if (disposed) { releaseHtmlAnswerLease(lease.leaseToken); return; }
    activeLease = lease.leaseToken;
    const selectedLocale = normalizeLocaleCode(lease.locale);
    if (selectedLocale === null) throw new HtmlAnswerViewerError("htmlAnswers.invalid");
    locale = selectedLocale;
    await ensureLocaleCatalog(locale);
    if (disposed) return;
    const remaining = lease.expiresInSeconds * 1000 - (performance.now() - receivedAt);
    if (remaining <= 0) throw new HtmlAnswerViewerError("htmlAnswers.expired");
    expiry = setTimeout(() => { release(); render({ status: "error", label: "htmlAnswers.expired" }); }, remaining);
    document.title = lease.title;
    render({ status: "ready", lease });
  } catch (error) {
    entryUrl.hash = "";
    release();
    render({ status: "error", label: error instanceof HtmlAnswerViewerError ? error.label : "htmlAnswers.unavailable" });
  }
}
void load();
