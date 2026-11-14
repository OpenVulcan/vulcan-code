import { requestCoreBrowser } from "@/service/core-browser-service";

// Snapshot coordinates are measured in CSS pixels of the exact projected page.
// 快照坐标以确切投影页面的 CSS 像素为单位。
export type BrowserDomViewport = {
  pageId: string;
  pageGeneration: string;
  viewportGeneration: string;
  width: number;
  height: number;
};

// An inert DOM revision and its full graph of captured resource identities.
// 惰性 DOM 修订及其完整的已捕获资源标识图。
export type BrowserDomRevision = {
  revision: string;
  documentId: string;
  html: string;
  assets: string[];
  scroll: [string, number, number][];
  windowScroll: [number, number];
  focused: string | null;
  // Only the UTF-16 length and native selection of password inputs cross the projection wire.
  // 密码框只有 UTF-16 长度和原生选区经过投影协议。
  passwords: [id: string, length: number, start: number, end: number, direction: "forward" | "backward" | "none"][];
  width: number;
  height: number;
};

// The bridge supplies exact generations separately from revision and asset payloads.
// 桥接层独立于修订和资源正文提供确切代次。
export type BrowserDomReply<T> = Omit<BrowserDomViewport, "width" | "height"> & { result: T | null };

// The host may return a compact replacement against the requested base revision.
// 宿主可返回针对请求基准修订的紧凑替换。
export type BrowserDomUpdate = BrowserDomRevision | { revision: string }
  | (Omit<BrowserDomRevision, "html"> & { patch: { start: number; end: number; text: string } });

// Resource bytes are bounded chunks of the original Playwright response.
// 资源字节是原始 Playwright 响应的有限分块。
type AssetChunk = { mimeType: string; dataBase64: string; dependencies: string[]; nextOffset: number | null };

// A private Blob cache avoids retransferring identical resources across snapshots of a page.
// 私有 Blob 缓存避免在同一页面的快照间重复传输相同资源。
export class BrowserDomAssets {
  private readonly urls = new Map<string, string>();
  private closed = false;

  // Load missing resources through the authenticated Core bridge and materialize their local URLs.
  // 通过已鉴权的 Core 桥接加载缺少的资源，并生成其本地 URL。
  async render(owner: string, sessionId: string, pageId: string, revision: BrowserDomRevision): Promise<string> {
    // Build binary URLs first, then stylesheets after their referenced URLs are available.
    // 先建立二进制 URL，再在其引用 URL 可用后建立样式表。
    const pending = new Map<string, { css: string; dependencies: string[] }>();
    for (const assetId of revision.assets) {
      if (this.closed) throw new Error("Browser DOM view closed.");
      if (this.urls.has(assetId)) continue;
      const chunks: Uint8Array<ArrayBuffer>[] = [];
      let offset = 0;
      let asset: AssetChunk | null;
      do {
        const reply = await requestCoreBrowser<BrowserDomReply<AssetChunk>>(owner, "browser.dom.asset", {
          sessionId, pageId, assetId, offset,
        });
        asset = reply.result;
        if (this.closed || reply.pageId !== pageId) throw new Error("Browser DOM view changed.");
        if (!asset) break;
        chunks.push(Uint8Array.from(atob(asset.dataBase64), character => character.charCodeAt(0)));
        if (asset.nextOffset !== null && asset.nextOffset <= offset) throw new Error("Invalid browser resource offset.");
        offset = asset.nextOffset ?? 0;
      } while (offset !== 0);
      if (!asset) continue;
      const body = new Blob(chunks, { type: asset.mimeType });
      if (asset.mimeType === "text/css") pending.set(assetId, { css: await body.text(), dependencies: asset.dependencies });
      else this.urls.set(assetId, URL.createObjectURL(body));
    }
    // Imports form a DAG because the host removes cyclic edges while rewriting.
    // 宿主在改写时移除循环边，因此导入组成有向无环图。
    while (!this.closed && pending.size) {
      const previous = pending.size;
      for (const [id, style] of pending) {
        if (style.dependencies.some(dependency => pending.has(dependency))) continue;
        this.urls.set(id, URL.createObjectURL(new Blob([this.substitute(style.css)], { type: "text/css" })));
        pending.delete(id);
      }
      if (pending.size === previous) throw new Error("Invalid browser stylesheet dependencies.");
    }
    if (this.closed) throw new Error("Browser DOM view closed.");
    const retained = new Set(revision.assets);
    for (const [id, url] of this.urls) if (!retained.has(id)) {
      URL.revokeObjectURL(url);
      this.urls.delete(id);
    }
    return this.substitute(revision.html);
  }

  // Release object URLs and stop issuing RPCs when the page is no longer observed.
  // 页面停止观察时，释放对象 URL 并停止发出 RPC。
  close(): void {
    this.closed = true;
    for (const url of this.urls.values()) URL.revokeObjectURL(url);
    this.urls.clear();
  }

  // Replace only content-addressed placeholders with local URLs under the viewer CSP.
  // 只将按内容寻址的占位符替换为查看器 CSP 允许的本地 URL。
  private substitute(text: string): string {
    return text.replace(/vulcan-resource:([a-f0-9]{64})/g, (_, id: string) => this.urls.get(id) ?? "data:,");
  }
}
