import { useState } from "react";
import { useI18n, type TranslationKey } from "@/i18n";
import type { HtmlAnswerLease } from "./transport";

/** Trusted viewer state; only a successfully exchanged lease may create an executing iframe.
 * 可信查看器状态；只有成功兑换的租约才可以创建执行 iframe。 */
export type HtmlAnswerViewerState = { status: "loading" }
  | { status: "error"; label: TranslationKey }
  | { status: "ready"; lease: HtmlAnswerLease };

/** Display one lease as an isolated preview or selectable source, with localized controls.
 * 将一个租约显示为隔离预览或可选择源码，并提供本地化控件。 */
export function HtmlAnswerViewer({ state }: { state: HtmlAnswerViewerState }) {
  const { t } = useI18n();
  const [showSource, setShowSource] = useState(false);
  const [copyFeedback, setCopyFeedback] = useState<TranslationKey | null>(null);
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
      : <iframe className="html-viewer-content" title={lease.title} src={lease.contentUrl}
        sandbox="allow-scripts" referrerPolicy="no-referrer" />}
  </main>;
}
