import * as React from "react";
import { Code, Copy, FileCode2, Maximize2, Minimize2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { formatMessage, useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n-messages";
import type { HtmlAnswerDescriptor } from "@/service/html-answer-projections";
import { HTML_ANSWER_CLOSE } from "@/service/html-answer-close";
import { createHtmlAnswerViewLink, htmlAnswerRequestErrorLabel, revokeHtmlAnswerViewLink, type HtmlAnswerViewLink } from "@/service/html-answers-service";
import type { HtmlAnswerMessageContext } from "./html-answer-context";
import "./html-answer-card.css";

/** Collapsed, nonexecuting result card. Only Core-ready descriptors enable a viewer request.
 * 收起且不执行页面的结果卡。仅 Core 就绪描述允许申请查看器。
 * Inputs preserve raw source and exact ownership; returns a card plus an explicitly opened dialog.
 * 输入保留原始源码和精确归属；返回卡片及明确操作打开的弹窗。 */
export function HtmlAnswerCard(props: {
  /** Canonical source authority for this message.
   * 当前消息的权威源码归属。 */
  message: HtmlAnswerMessageContext;
  /** Durable description, absent for streaming or not-yet-saved candidates.
   * 可靠描述；流式或尚未保存的候选没有描述。 */
  descriptor?: HtmlAnswerDescriptor;
  /** Exact original source; unavailable candidates retain the complete original fence.
   * 精确原始源码；不可用候选保留完整原始围栏。 */
  source: string;
  /** Whether the authoritative assistant event is still growing.
   * 权威助手事件是否仍在增长。 */
  streaming: boolean;
  /** Existing clipboard adapter; failures must reject.
   * 既有剪贴板适配器；失败必须拒绝。 */
  copyText: (text: string) => Promise<void>;
}) {
  const { t, locale } = useI18n();
  const [open, setOpen] = React.useState(false);
  const [fullscreen, setFullscreen] = React.useState(false);
  const [showSource, setShowSource] = React.useState(false);
  const [link, setLink] = React.useState<HtmlAnswerViewLink | null>(null);
  const [pending, setPending] = React.useState(false);
  const [feedback, setFeedback] = React.useState<TranslationKey | null>(null);
  // The displayed link lifetime is supplied by Core, never duplicated as a client constant.
  // 链接有效期由 Core 提供，绝不重复定义为客户端常量。
  const [copiedTicketSeconds, setCopiedTicketSeconds] = React.useState(0);
  // The operation token invalidates delayed opens after close, owner change or unmount.
  // 操作令牌使关闭、归属变化或卸载后的迟到打开失效。
  const operation = React.useRef<object | null>(null);
  const ownedGrant = React.useRef<string | null>(null);
  const mounted = React.useRef(false);
  // Retain the exact trigger and viewer for focus restoration and sender authentication.
  // 保留精确触发按钮与查看器，用于焦点恢复及发送者认证。
  const trigger = React.useRef<HTMLButtonElement>(null);
  const viewer = React.useRef<HTMLIFrameElement>(null);
  const title = props.descriptor?.title ?? t("htmlAnswers.label");
  const ready = props.descriptor?.availability.status === "ready";
  React.useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      operation.current = null;
      if (ownedGrant.current) void revokeHtmlAnswerViewLink(ownedGrant.current).catch(() => undefined);
      ownedGrant.current = null;
    };
  }, []);

  /** Close immediately and invalidate both an in-flight issuance and an already exchanged grant.
   * 立即关闭，同时使签发中的操作和已兑换授权失效。 */
  const close = React.useCallback(() => {
    operation.current = null;
    setOpen(false);
    setPending(false);
    setLink(null);
    const grant = ownedGrant.current;
    ownedGrant.current = null;
    if (grant) void revokeHtmlAnswerViewLink(grant).catch(() => {
      if (mounted.current) setFeedback("htmlAnswers.unavailable");
    });
  }, []);

  React.useEffect(() => {
    if (!open || !link) return;
    const origin = new URL(link.viewerUrl).origin;
    /** Accept only this trusted viewer's close signal; no generated-page host API is exposed.
     * 仅接受当前可信查看器的关闭信号；不向生成页面暴露宿主 API。 */
    const receiveClose = (event: MessageEvent) => {
      if (event.data === HTML_ANSWER_CLOSE && event.origin === origin
        && viewer.current?.contentWindow && event.source === viewer.current.contentWindow) close();
    };
    window.addEventListener("message", receiveClose);
    return () => window.removeEventListener("message", receiveClose);
  }, [open, link, close]);

  /** Issue only on a user click; obsolete responses are revoked instead of opening another owner.
   * 仅在用户点击时签发；过期回包会撤销，绝不打开其他归属。
   * copy selects clipboard delivery; returns after issuance and the selected action finish.
   * copy 选择剪贴板交付；签发及对应操作结束后返回。 */
  const requestView = async (copy: boolean) => {
    const availability = props.descriptor?.availability;
    if (availability?.status !== "ready" || operation.current) return;
    const token = {};
    operation.current = token;
    setPending(true);
    setFeedback(null);
    if (!copy) { setOpen(true); setFullscreen(false); }
    let copying = false;
    try {
      const result = await createHtmlAnswerViewLink({ ...props.message.source, artifactId: availability.artifact_id }, locale);
      if (!mounted.current || operation.current !== token) {
        await revokeHtmlAnswerViewLink(result.grantId);
        return;
      }
      if (copy) {
        copying = true;
        try {
          await props.copyText(result.viewerUrl);
          if (mounted.current && operation.current === token) {
            setCopiedTicketSeconds(result.expiresInSeconds);
            setFeedback("htmlAnswers.linkCopied");
          }
        } catch {
          await revokeHtmlAnswerViewLink(result.grantId);
          throw new Error("Clipboard unavailable");
        }
      } else {
        ownedGrant.current = result.grantId;
        setLink(result);
      }
    } catch (error) {
      if (mounted.current && operation.current === token) {
        setOpen(false);
        setFeedback(copying ? "htmlAnswers.copyFailed" : htmlAnswerRequestErrorLabel(error));
      }
    } finally {
      if (mounted.current && operation.current === token) {
        operation.current = null;
        setPending(false);
      }
    }
  };

  return (
    <figure className="html-answer-card" data-status={ready ? "ready" : props.streaming ? "generating" : "unavailable"}>
      <figcaption><FileCode2 aria-hidden="true" size={18} /><strong>{title}</strong></figcaption>
      {!ready && <p role="status">{t(htmlAnswerStatusLabel(props.descriptor, props.streaming))}</p>}
      <div className="html-answer-card-actions">
        <Button ref={trigger} variant="outline" size="sm" disabled={!ready || pending} onClick={() => void requestView(false)}>{t("htmlAnswers.open")}</Button>
        {ready && <Button variant="ghost" size="sm" disabled={pending} onClick={() => void requestView(true)}>{t("htmlAnswers.copyLink")}</Button>}
        <Button variant="ghost" size="sm" aria-expanded={showSource} onClick={() => setShowSource(!showSource)}><Code size={14} />{t("htmlAnswers.source")}</Button>
        <Button variant="ghost" size="sm" onClick={() => {
          void props.copyText(props.source).then(() => {
            if (mounted.current) setFeedback("htmlAnswers.copied");
          }, () => { if (mounted.current) { setShowSource(true); setFeedback("htmlAnswers.copyFailed"); } });
        }}><Copy size={14} />{t("htmlAnswers.copySource")}</Button>
      </div>
      {feedback && <p role="status">{feedback === "htmlAnswers.linkCopied" ? formatMessage(t(feedback), { seconds: copiedTicketSeconds }) : t(feedback)}</p>}
      {showSource && <pre className="html-answer-source"><code>{props.source}</code></pre>}
      <Dialog open={open} onOpenChange={(value) => { if (!value) close(); }}>
        <DialogContent size={fullscreen ? "fullscreen" : "viewer"} layout="viewer" className="html-answer-dialog" aria-describedby={undefined}
          onCloseAutoFocus={(event) => { event.preventDefault(); trigger.current?.focus(); }}>
          <div className="html-answer-dialog-header">
            <DialogTitle>{t("htmlAnswers.label")}</DialogTitle>
            <Button variant="ghost" size="icon-sm" aria-label={t(fullscreen ? "htmlAnswers.restore" : "htmlAnswers.maximize")} onClick={() => setFullscreen(!fullscreen)}>
              {fullscreen ? <Minimize2 size={16} /> : <Maximize2 size={16} />}
            </Button>
          </div>
          {open && link ? <iframe ref={viewer} title={title} src={link.viewerUrl} allow="clipboard-write" referrerPolicy="no-referrer" className="html-answer-viewer-frame" />
            : <p role="status">{t("ui.loading")}</p>}
        </DialogContent>
      </Dialog>
    </figure>
  );
}

/** Map Core rejection categories to shared copy; never infer executable validity in the UI.
 * 将 Core 拒绝分类映射到共享文案；绝不在界面推断可执行有效性。 */
function htmlAnswerStatusLabel(descriptor: HtmlAnswerDescriptor | undefined, streaming: boolean): TranslationKey {
  if (streaming) return "htmlAnswers.generating";
  if (descriptor?.availability.status !== "unavailable") return "htmlAnswers.waiting";
  switch (descriptor.availability.reason) {
    case "not_persisted": return "htmlAnswers.waiting";
    case "interrupted": case "unclosed_fence": return "htmlAnswers.interrupted";
    case "page_too_large": case "too_many_pages": case "total_too_large": return "htmlAnswers.tooLarge";
    default: return "htmlAnswers.invalidFormat";
  }
}
