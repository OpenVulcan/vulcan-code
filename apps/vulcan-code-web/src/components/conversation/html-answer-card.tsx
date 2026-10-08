import * as React from "react";
import { Code, Copy, FileCode2, Link2, Maximize2, Minimize2, RotateCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { formatMessage, useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n-messages";
import { HTML_ANSWER_CLOSE_MESSAGE, HTML_ANSWER_HEIGHT_MESSAGE } from "@/generated/protocol-limits";
import type { HtmlAnswerDescriptor } from "@/service/html-answer-projections";
import { createHtmlAnswerViewLink, htmlAnswerRequestErrorLabel, revokeHtmlAnswerViewLink, type HtmlAnswerViewLink } from "@/service/html-answers-service";
import { STREAMING_MARKDOWN_RAW_TAIL_MAX_CHARS } from "./streaming-markdown-blocks";
import type { HtmlAnswerMessageContext } from "./html-answer-context";
import "./html-answer-card.css";

// Inline frame height bounds in CSS pixels; taller pages scroll inside the frame or use maximize.
// 内嵌框架的高度范围（CSS 像素）；更高的页面在框架内滚动或使用最大化。
const FRAME_MIN_HEIGHT = 120;
const FRAME_MAX_HEIGHT = 4000;
const FRAME_INITIAL_HEIGHT = 280;
// Mount once the card comes within this margin of the viewport, release beyond the wider one.
// 卡片进入视口此边距内时挂载，超出更宽边距后释放。
const MOUNT_MARGIN = "50% 0px";
const RELEASE_MARGIN = "200% 0px";

/** Inline result card: streams the draft source, then renders the Core-authorized page in place.
 * 内嵌结果卡：先流式显示草稿源码，再就地渲染经 Core 授权的页面。
 * Only Core-ready descriptors request a viewer, and only while the card is near the viewport;
 * the page runs in the isolated viewer frame and never in the workbench document.
 * 仅 Core 就绪描述且卡片接近视口时才申请查看器；页面运行于隔离查看器框架，绝不进入工作台文档。 */
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
  const [maximized, setMaximized] = React.useState(false);
  const [showSource, setShowSource] = React.useState(false);
  const [link, setLink] = React.useState<HtmlAnswerViewLink | null>(null);
  const [pending, setPending] = React.useState(false);
  const [failure, setFailure] = React.useState<TranslationKey | null>(null);
  const [feedback, setFeedback] = React.useState<TranslationKey | null>(null);
  const [height, setHeight] = React.useState(FRAME_INITIAL_HEIGHT);
  const [near, setNear] = React.useState(false);
  // The displayed link lifetime is supplied by Core, never duplicated as a client constant.
  // 链接有效期由 Core 提供，绝不重复定义为客户端常量。
  const [copiedTicketSeconds, setCopiedTicketSeconds] = React.useState(0);
  const [copyPending, setCopyPending] = React.useState(false);
  // Operation tokens invalidate late grants after release, owner change or unmount; the inline
  // frame and a copied link are independent grants.
  // 操作令牌使释放、归属变化或卸载后的迟到授权失效；内嵌框架与复制链接是相互独立的授权。
  const viewOperation = React.useRef<object | null>(null);
  const copyOperation = React.useRef<object | null>(null);
  const ownedGrant = React.useRef<string | null>(null);
  const mounted = React.useRef(false);
  const card = React.useRef<HTMLElement>(null);
  const viewer = React.useRef<HTMLIFrameElement>(null);
  const streamTail = React.useRef<HTMLPreElement>(null);
  const title = props.descriptor?.title ?? t("htmlAnswers.label");
  const availability = props.descriptor?.availability;
  const ready = availability?.status === "ready";

  /** Drop the frame and revoke its grant; late issuance for the old token is revoked on arrival.
   * 移除框架并撤销其授权；旧令牌的迟到签发在到达时撤销。 */
  const release = React.useCallback(() => {
    viewOperation.current = null;
    setPending(false);
    setLink(null);
    const grant = ownedGrant.current;
    ownedGrant.current = null;
    if (grant) void revokeHtmlAnswerViewLink(grant).catch(() => undefined);
  }, []);

  React.useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      viewOperation.current = null;
      copyOperation.current = null;
      if (ownedGrant.current) void revokeHtmlAnswerViewLink(ownedGrant.current).catch(() => undefined);
      ownedGrant.current = null;
    };
  }, []);

  // Track viewport proximity with hysteresis so scrolling past does not churn single-use tickets.
  // 以滞后区间跟踪视口距离，避免滚动经过时反复消耗单次票据。
  React.useEffect(() => {
    const element = card.current;
    if (!element || !ready) return;
    if (typeof IntersectionObserver === "undefined") { setNear(true); return; }
    const mount = new IntersectionObserver(([entry]) => { if (entry?.isIntersecting) setNear(true); }, { rootMargin: MOUNT_MARGIN });
    const keep = new IntersectionObserver(([entry]) => { if (entry && !entry.isIntersecting) setNear(false); }, { rootMargin: RELEASE_MARGIN });
    mount.observe(element);
    keep.observe(element);
    return () => { mount.disconnect(); keep.disconnect(); };
  }, [ready]);

  /** Issue one grant for `copy` delivery or for the inline frame; obsolete responses are revoked.
   * 为复制交付或内嵌框架签发一份授权；过期回包会被撤销。 */
  const { copyText, message: { source: owner } } = props;
  const request = React.useCallback(async (copy: boolean) => {
    const operation = copy ? copyOperation : viewOperation;
    if (availability?.status !== "ready" || operation.current) return;
    const token = {};
    operation.current = token;
    if (copy) { setCopyPending(true); setFeedback(null); } else { setPending(true); setFailure(null); }
    let delivering = false;
    try {
      const result = await createHtmlAnswerViewLink({ ...owner, artifactId: availability.artifact_id }, locale);
      if (!mounted.current || operation.current !== token) {
        await revokeHtmlAnswerViewLink(result.grantId);
        return;
      }
      if (copy) {
        delivering = true;
        try {
          await copyText(result.viewerUrl);
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
        if (copy) setFeedback(delivering ? "htmlAnswers.copyFailed" : htmlAnswerRequestErrorLabel(error));
        else setFailure(htmlAnswerRequestErrorLabel(error));
      }
    } finally {
      if (mounted.current && operation.current === token) {
        operation.current = null;
        if (copy) setCopyPending(false); else setPending(false);
      }
    }
  }, [availability, locale, owner, copyText]);

  // Render automatically once ready and near the viewport; release when it moves far away.
  // 就绪且接近视口时自动渲染；远离视口后释放。
  React.useEffect(() => {
    if (!ready) return;
    if (near && !link && !pending && !failure) void request(false);
    if (!near && (link || pending) && !maximized) release();
  }, [ready, near, link, pending, failure, maximized, request, release]);

  // Size the frame from the trusted viewer's relay and restore from maximize on its close signal.
  // 依据可信查看器的转发调整框架高度，并在收到关闭信号时退出最大化。
  React.useEffect(() => {
    if (!link) return;
    const origin = new URL(link.viewerUrl).origin;
    /** Accept only this viewer window's messages; the page can report a height or ask to close.
     * 仅接受当前查看器窗口的消息；页面只能上报高度或请求关闭。 */
    const receive = (event: MessageEvent) => {
      if (event.origin !== origin || !viewer.current?.contentWindow || event.source !== viewer.current.contentWindow) return;
      if (event.data === HTML_ANSWER_CLOSE_MESSAGE) { setMaximized(false); return; }
      const data = event.data as { type?: unknown; height?: unknown } | null;
      if (data?.type === HTML_ANSWER_HEIGHT_MESSAGE && typeof data.height === "number" && Number.isFinite(data.height)) {
        setHeight(Math.min(FRAME_MAX_HEIGHT, Math.max(FRAME_MIN_HEIGHT, Math.ceil(data.height))));
      }
    };
    window.addEventListener("message", receive);
    return () => window.removeEventListener("message", receive);
  }, [link]);

  // Maximize promotes the card itself to the top layer: transformed transcript ancestors cannot
  // offset it, and the frame is never moved, so the page keeps its state.
  // 最大化将卡片自身提升到顶层：带变换的会话祖先无法使其偏移，框架也不会移动，页面状态得以保留。
  React.useLayoutEffect(() => {
    const element = card.current;
    if (!element || typeof element.showPopover !== "function") return;
    if (maximized) {
      element.setAttribute("popover", "manual");
      element.showPopover();
      return () => { if (element.matches(":popover-open")) element.hidePopover(); element.removeAttribute("popover"); };
    }
  }, [maximized]);

  React.useEffect(() => {
    if (!maximized) return;
    const keydown = (event: KeyboardEvent) => { if (event.key === "Escape") setMaximized(false); };
    window.addEventListener("keydown", keydown);
    return () => window.removeEventListener("keydown", keydown);
  }, [maximized]);

  // Keep the newest streamed lines in view while the draft grows.
  // 草稿增长时保持最新流式内容可见。
  React.useLayoutEffect(() => {
    if (streamTail.current) streamTail.current.scrollTop = streamTail.current.scrollHeight;
  }, [props.source]);

  /** Re-render from a fresh grant after an error, an expired lease or a user request.
   * 出错、租约过期或用户请求后使用新授权重新渲染。 */
  const reload = () => { release(); setFailure(null); };

  const status = ready ? null : htmlAnswerStatusLabel(props.descriptor, props.streaming);
  const preview = !ready && (props.streaming || status === "htmlAnswers.waiting");
  return (
    <figure ref={card} className="html-answer-card" data-status={ready ? "ready" : props.streaming ? "generating" : "unavailable"}
      data-maximized={maximized || undefined}>
      <figcaption className="html-answer-card-bar">
        <FileCode2 aria-hidden="true" size={16} />
        <strong>{title}</strong>
        {status && <span role="status" className="html-answer-card-status">{t(status)}</span>}
        <span className="html-answer-card-actions">
          <Button variant="ghost" size="icon-sm" aria-pressed={showSource} aria-label={t("htmlAnswers.source")} title={t("htmlAnswers.source")}
            onClick={() => setShowSource(!showSource)}><Code size={15} /></Button>
          <Button variant="ghost" size="icon-sm" aria-label={t("htmlAnswers.copySource")} title={t("htmlAnswers.copySource")} onClick={() => {
            void props.copyText(props.source).then(() => {
              if (mounted.current) setFeedback("htmlAnswers.copied");
            }, () => { if (mounted.current) { setShowSource(true); setFeedback("htmlAnswers.copyFailed"); } });
          }}><Copy size={15} /></Button>
          {ready && <>
            <Button variant="ghost" size="icon-sm" aria-label={t("htmlAnswers.copyLink")} title={t("htmlAnswers.copyLink")}
              disabled={copyPending} onClick={() => void request(true)}><Link2 size={15} /></Button>
            <Button variant="ghost" size="icon-sm" aria-label={t("htmlAnswers.reload")} title={t("htmlAnswers.reload")}
              disabled={pending} onClick={reload}><RotateCw size={15} /></Button>
            <Button variant="ghost" size="icon-sm" aria-label={t(maximized ? "htmlAnswers.restore" : "htmlAnswers.maximize")}
              title={t(maximized ? "htmlAnswers.restore" : "htmlAnswers.maximize")} onClick={() => setMaximized(!maximized)}>
              {maximized ? <Minimize2 size={15} /> : <Maximize2 size={15} />}
            </Button>
          </>}
        </span>
      </figcaption>
      {feedback && <p role="status" className="html-answer-card-feedback">
        {feedback === "htmlAnswers.linkCopied" ? formatMessage(t(feedback), { seconds: copiedTicketSeconds }) : t(feedback)}
      </p>}
      {preview && !showSource && <pre ref={streamTail} className="html-answer-stream" aria-live="off"><code>
        {streamedDraft(props.source)}
      </code></pre>}
      {showSource && <pre className="html-answer-source"><code>{props.source}</code></pre>}
      {ready && <div className="html-answer-render" hidden={showSource} style={maximized ? undefined : { height }}>
        {link ? <iframe ref={viewer} title={title} src={link.viewerUrl} allow="clipboard-write" referrerPolicy="no-referrer"
          className="html-answer-viewer-frame" />
          : <div className="html-answer-render-state" role={failure ? "alert" : "status"}>
            {failure ? <>{t(failure)} <Button variant="outline" size="sm" onClick={reload}>{t("htmlAnswers.reload")}</Button></> : t("ui.loading")}
          </div>}
      </div>}
    </figure>
  );
}

/** Return the bounded display tail of a growing fence without its marker lines; text only, never parsed.
 * 返回增长中围栏去除标记行后的有界展示尾部；仅为文字，绝不解析。 */
function streamedDraft(source: string): string {
  const body = source.replace(/^(?:\r?\n)* {0,3}`{3,}[^\n]*\n?/u, "").replace(/\n {0,3}`{3,}[ \t]*\r?\n?$/u, "");
  return body.length > STREAMING_MARKDOWN_RAW_TAIL_MAX_CHARS ? body.slice(-STREAMING_MARKDOWN_RAW_TAIL_MAX_CHARS) : body;
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
