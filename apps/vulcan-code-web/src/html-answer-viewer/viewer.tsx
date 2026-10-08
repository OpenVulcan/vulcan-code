import { useEffect, useRef, useState } from "react";
import { useI18n, type TranslationKey } from "@/i18n";
import { HTML_ANSWER_CLOSE_MESSAGE, HTML_ANSWER_HEIGHT_MESSAGE } from "@/generated/protocol-limits";
import type { HtmlAnswerLease } from "./transport";

/** Trusted viewer state; only a successfully exchanged lease may create an executing iframe.
 * 可信查看器状态；只有成功兑换的租约才可以创建执行 iframe。 */
export type HtmlAnswerViewerState = { status: "loading" }
  | { status: "error"; label: TranslationKey }
  | { status: "ready"; lease: HtmlAnswerLease };

/** Display one lease as an isolated preview or selectable source, with localized controls.
 * 将一个租约显示为隔离预览或可选择源码，并提供本地化控件。
 * Embedded in the workbench, only the page frame is shown; the conversation card owns the controls.
 * 嵌入工作台时只显示页面框架；控件由会话卡片提供。 */
export function HtmlAnswerViewer({ state }: { state: HtmlAnswerViewerState }) {
  const embedded = window.parent !== window;
  const { t } = useI18n();
  const [showSource, setShowSource] = useState(false);
  const [copyFeedback, setCopyFeedback] = useState<TranslationKey | null>(null);
  // Only the current opaque child may relay Escape; standalone viewers never close their browser.
  // 仅当前不透明子框架可转发 Esc；独立查看器绝不关闭浏览器。
  const content = useRef<HTMLIFrameElement>(null);
  useEffect(() => {
    /** Relay the sole non-sensitive notification to the embedding workbench, if present.
     * 如存在嵌入工作台，转发唯一不含敏感信息的通知。 */
    const requestClose = () => {
      if (window.parent !== window) window.parent.postMessage(HTML_ANSWER_CLOSE_MESSAGE, "*");
    };
    /** Authenticate both the child Window and its sandboxed origin, then relay only a close signal
     * or a finite content height rebuilt from the number alone.
     * 同时认证子窗口与其沙箱来源，然后只转发关闭信号或仅由数值重建的有限内容高度。 */
    const receiveClose = (event: MessageEvent) => {
      if (event.origin !== "null" || !content.current?.contentWindow || event.source !== content.current.contentWindow) return;
      if (event.data === HTML_ANSWER_CLOSE_MESSAGE) { requestClose(); return; }
      const data = event.data as { type?: unknown; height?: unknown } | null;
      if (window.parent !== window && data?.type === HTML_ANSWER_HEIGHT_MESSAGE
        && typeof data.height === "number" && Number.isFinite(data.height)) {
        window.parent.postMessage({ type: HTML_ANSWER_HEIGHT_MESSAGE, height: data.height }, "*");
      }
    };
    /** Close an embedded viewer when Escape originates in its own trusted controls.
     * Esc 来自可信控件时关闭嵌入查看器。 */
    const keydown = (event: KeyboardEvent) => { if (event.key === "Escape") requestClose(); };
    window.addEventListener("message", receiveClose);
    window.addEventListener("keydown", keydown);
    return () => { window.removeEventListener("message", receiveClose); window.removeEventListener("keydown", keydown); };
  }, []);
  if (state.status !== "ready") {
    return <main className="html-viewer-state" role={state.status === "error" ? "alert" : "status"}>
      {t(state.status === "loading" ? "ui.loading" : state.label)}
    </main>;
  }
  const { lease } = state;
  /** Copy original source after an explicit click; keep a selectable source view on denial.
   * 显式点击后复制原始源码；被拒绝时保留可选择源码视图。 */
  const copySource = async () => {
    try { await navigator.clipboard.writeText(lease.source); setCopyFeedback("htmlAnswers.copied"); }
    catch { setShowSource(true); setCopyFeedback("htmlAnswers.copyFailed"); }
  };
  if (embedded) {
    return <main className="html-viewer html-viewer-embedded">
      <iframe ref={content} className="html-viewer-content" title={lease.title} src={lease.contentUrl}
        sandbox="allow-scripts" referrerPolicy="no-referrer" />
    </main>;
  }
  return <main className="html-viewer">
    <header className="html-viewer-toolbar">
      <h1>{lease.title}</h1>
      <div className="html-viewer-actions">
        <button type="button" aria-pressed={!showSource} onClick={() => setShowSource(false)}>{t("htmlAnswers.preview")}</button>
        <button type="button" aria-pressed={showSource} onClick={() => setShowSource(true)}>{t("htmlAnswers.source")}</button>
        <button type="button" onClick={() => void copySource()}>{t("htmlAnswers.copySource")}</button>
      </div>
    </header>
    {copyFeedback && <p className="html-viewer-feedback" role="status">{t(copyFeedback)}</p>}
    {showSource ? <pre className="html-viewer-source" tabIndex={0}><code>{lease.source}</code></pre>
      : <iframe ref={content} className="html-viewer-content" title={lease.title} src={lease.contentUrl}
        sandbox="allow-scripts" referrerPolicy="no-referrer" />}
  </main>;
}
