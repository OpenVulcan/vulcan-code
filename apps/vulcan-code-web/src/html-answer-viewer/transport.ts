import type { TranslationKey } from "@/i18n-messages";

/** A one-result ticket parsed from the trusted viewer URL, never a Core instance token.
 * 从可信查看器 URL 解析的单结果票据，绝不是 Core 实例令牌。 */
export interface HtmlAnswerTicket {
  /** Exact session and artifact route ownership.
   * 精确的会话及结果路由归属。 */
  sessionId: string;
  artifactId: string;
  /** Single-use fragment capability, kept only for the exchange request.
   * 仅为兑换请求保留的单次片段能力。 */
  ticket: string;
}

/** Scoped lease returned by the trusted Core exchange endpoint.
 * 可信 Core 兑换端点返回的限定租约。 */
export interface HtmlAnswerLease {
  /** Isolated read-only renderer URL; its capability cannot invoke Core RPC.
   * 隔离只读渲染地址；其能力不能调用 Core RPC。 */
  contentUrl: string;
  /** Bearer used only to release this page's lease.
   * 仅用于释放当前页面租约的持有者秘密。 */
  leaseToken: string;
  /** Remaining monotonic lease lifetime supplied by Core.
   * Core 提供的剩余单调租约有效秒数。 */
  expiresInSeconds: number;
  /** Original source and plain-text title, never injected into trusted DOM as markup.
   * 原始源码与纯文本标题，绝不作为标记注入可信 DOM。 */
  source: string;
  title: string;
  /** Exact locale from the shared registry selected by the issuing client.
   * 签发客户端选择的共享注册表精确语言。 */
  locale: string;
}

/** Stable public failure category; error messages never contain URLs, tokens or original response text.
 * 稳定公开失败分类；错误消息绝不包含 URL、令牌或原始响应文本。 */
export class HtmlAnswerViewerError extends Error {
  /** Shared-catalog label identifying the rejected viewing operation.
   * 标识被拒绝查看操作的共享目录文案键。 */
  readonly label: TranslationKey;
  /** Construct one sanitized error from a shared label, returning the error instance.
   * 根据共享文案键构造一份已清理错误，返回错误实例。 */
  constructor(label: TranslationKey) { super(label); this.label = label; }
}

/** Read the exact path and one-use fragment of `url`; reject malformed links without echoing them.
 * 读取 `url` 的精确路径和单次片段；拒绝格式错误的链接且不回显链接。 */
export function parseHtmlAnswerTicket(url: URL): HtmlAnswerTicket {
  const route = /^\/html-answers\/([^/]+)\/([^/]+)$/u.exec(url.pathname);
  const fragment = new URLSearchParams(url.hash.slice(1));
  const ticket = fragment.get("ticket");
  if (!route || !ticket || [...fragment.keys()].length !== 1 || url.search) {
    throw new HtmlAnswerViewerError("htmlAnswers.invalid");
  }
  return { sessionId: route[1], artifactId: route[2], ticket };
}

/** Exchange a ticket without cookies or general Core credentials; propagate only stable error categories.
 * 不携带 Cookie 或通用 Core 凭据兑换票据；仅传播稳定错误分类。 */
export async function exchangeHtmlAnswerTicket(ticket: HtmlAnswerTicket, signal: AbortSignal): Promise<HtmlAnswerLease> {
  const response = await fetch("/v1/html-answers/exchange", {
    method: "POST", credentials: "omit", cache: "no-store", redirect: "error", referrerPolicy: "no-referrer",
    headers: { "Content-Type": "application/json" }, body: JSON.stringify(ticket), signal,
  });
  if (!response.ok) {
    const failure = await response.json() as { error: string };
    throw new HtmlAnswerViewerError(htmlAnswerErrorLabel(failure.error));
  }
  return await response.json() as HtmlAnswerLease;
}

/** Release exactly `leaseToken` using a bounded keepalive POST; errors never escape unload handlers.
 * 使用有界 keepalive POST 精确释放 `leaseToken`；错误绝不逃出卸载处理器。 */
export function releaseHtmlAnswerLease(leaseToken: string): void {
  void fetch("/v1/html-answers/release", {
    method: "POST", credentials: "omit", cache: "no-store", redirect: "error", referrerPolicy: "no-referrer",
    headers: { "Content-Type": "application/json" }, body: JSON.stringify({ leaseToken }), keepalive: true,
  }).catch(() => undefined);
}

/** Translate stable server codes into shared user labels; unknown transport failures remain explicit.
 * 将稳定服务端错误码映射为共享用户文案；未知传输失败保持显式。 */
export function htmlAnswerErrorLabel(code: string): TranslationKey {
  switch (code) {
    case "expired": return "htmlAnswers.expired";
    case "stale_source": return "htmlAnswers.stale";
    case "invalid_request": return "htmlAnswers.invalid";
    case "capacity": case "rate_limited": return "htmlAnswers.busy";
    default: return "htmlAnswers.unavailable";
  }
}
