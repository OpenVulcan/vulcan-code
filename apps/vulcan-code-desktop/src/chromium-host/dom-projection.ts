import { createHash, randomUUID } from 'node:crypto';
import { parse as parseHtml, serialize, type DefaultTreeAdapterTypes } from 'parse5';
import { generate, parse as parseCss, walk } from 'css-tree';
import type { BrowserContext, CDPSession, Page } from 'playwright-core';

// Bound response capture, projection memory and each RPC message independently.
// 分别限制响应捕获、投影内存和每条 RPC 消息的大小。
const ASSET_LIMIT = 8 * 1024 * 1024;
const STORE_LIMIT = 64 * 1024 * 1024;
const ASSET_COUNT_LIMIT = 1200;
const MARKUP_LIMIT = 2 * 1024 * 1024;
const CHUNK_SIZE = 256 * 1024;
const REVISION_HISTORY_LIMIT = 4;
const IMAGE_TYPE = /^image\/(?:png|jpeg|webp|gif|avif|x-icon|vnd\.microsoft\.icon|bmp|apng|svg\+xml)$/;
const FONT_TYPE = /^(?:font\/(?:woff2?|ttf|otf|collection)|application\/(?:font-woff2?|vnd\.ms-fontobject|x-font-ttf|x-font-opentype|octet-stream))$/;
const PRESENTATION_ATTRS = new Set(['fill', 'stroke', 'clip-path', 'mask', 'filter', 'marker-start', 'marker-mid', 'marker-end', 'cursor']);
const STRIP_ATTRS = new Set(['srcset', 'imagesrcset', 'crossorigin', 'integrity', 'srcdoc', 'autofocus', 'nonce', 'ping', 'action', 'formaction', 'target', 'download', 'is', 'attributionsrc']);
const REMOVE_TAGS = new Set(['script', 'noscript', 'iframe', 'frame', 'object', 'embed', 'base', 'meta', 'portal', 'fencedframe', 'template', 'foreignobject', 'animate', 'set', 'animatemotion', 'animatetransform']);

// Serialized state uses UTF-16 offsets for editable text fields.
// 序列化状态使用 UTF-16 偏移量表示可编辑文本域的位置。
type EditRange = { before: string; start: number; end: number };
type Asset = { mimeType: string; body: Buffer; dependencies: string[] };
type CapturedAsset = { id: string; kind: string; url: string; sequence: number };
// Keep assets separated by committed loader and retain recent loader indexes for history restores.
// 按已提交的加载器隔离资源，并保留近期加载器的索引以支持历史恢复。
type PageAssets = { requests: Map<string, CapturedAsset>; loader: string; published: Set<string>;
  markup: Map<string, string>; documentId: string | null; blobs: Map<string, string>;
  initialStylesheetRedirects: Map<string, string> | null };
// A network response belongs to one loader and its redirect aliases.
// 一个网络响应归属一个加载器及其重定向别名。
type ResourceRequest = { loader: string; urls: string[]; kind: string; sequence: number; mimeType?: string };
type SourceNodes = { id: string; ids: WeakMap<Element, string>; next: number };
// Password geometry uses only native UTF-16 length and selection offsets.
// 密码框几何信息仅包含原生 UTF-16 长度与选区偏移。
type PasswordGeometry = [id: string, length: number, start: number, end: number, direction: 'forward' | 'backward' | 'none'];

// Stable element identities survive repeated snapshots of the same document.
// 稳定的元素标识在同一文档的多次快照中保持不变。
type ProjectionWindow = Window & { __vulcanProjectionNodes?: SourceNodes };

// Collect the live document and viewport state inside the browser. The host sanitizes the result.
// 在浏览器中采集实时文档和视口状态，由宿主清理采集结果。
async function readDocument({ documentId, blobLimit, knownBlobs }: {
  documentId: string; blobLimit: number; knownBlobs: { documentId: string | null; urls: string[] };
}) {
  const root = document.documentElement;
  if (!root || !document.body) return null;
  const scope = window as ProjectionWindow;
  const nodes = scope.__vulcanProjectionNodes ??= { id: documentId, ids: new WeakMap(), next: 0 };
  const copy = root.cloneNode(false) as Element;
  const scroll: [string, number, number][] = [];
  // Viewers reconstruct placeholders from this metadata without receiving password characters.
  // 查看端仅通过这些元数据重建占位符，不接收密码字符。
  const passwords: PasswordGeometry[] = [];
  const blobImages = new Set<string>();
  const known = new Set(knownBlobs.documentId === nodes.id ? knownBlobs.urls : []);
  const hover = new Set<Element>(Array.from(document.querySelectorAll(':hover')));
  let focus = document.activeElement;
  while (focus?.shadowRoot?.activeElement) focus = focus.shadowRoot.activeElement;
  let focused: string | null = null;

  // Serialize constructed sheets as marked styles for the viewer to adopt into the original scope.
  // 将构造样式表序列化为带标记的样式，供查看端在原作用域内采纳。
  function appendAdopted(source: Document | ShadowRoot, destination: Node): void {
    for (const sheet of source.adoptedStyleSheets) {
      if (sheet.disabled) continue;
      const style = document.createElement('style');
      style.setAttribute('data-vulcan-adopted', '');
      style.media = sheet.media.mediaText;
      style.textContent = Array.from(sheet.cssRules, rule => rule.cssText).join('\n').replace(/<\/style/gi, '\\3c /style');
      destination.appendChild(style);
    }
  }

  // Clone light DOM and open shadow trees with stable element IDs and source interaction state.
  // 克隆普通 DOM 和开放 Shadow DOM，附上稳定元素标识和源端交互状态。
  function projectElement(element: Element, mirror: Element): void {
    let id = nodes.ids.get(element);
    if (!id) {
      id = String(++nodes.next);
      nodes.ids.set(element, id);
    }
    mirror.removeAttribute('data-vulcan-shadow');
    mirror.removeAttribute('data-vulcan-adopted');
    mirror.toggleAttribute('data-vulcan-hover', hover.has(element));
    mirror.setAttribute('data-vulcan-node', id);
    if (element.scrollLeft || element.scrollTop) scroll.push([id, element.scrollLeft, element.scrollTop]);
    if (element === focus) focused = id;
    if (element instanceof HTMLInputElement) {
      if (element.type === 'password') {
        mirror.removeAttribute('value');
        // Read the native selection for this control while the renderer owns its value.
        // 在渲染器持有密码值期间读取控件的原生选区。
        const { selectionStart, selectionEnd, selectionDirection } = element;
        if (selectionStart !== null && selectionEnd !== null && selectionDirection !== null) {
          passwords.push([id, element.value.length, selectionStart, selectionEnd, selectionDirection]);
        }
      } else if (element.type === 'file') mirror.removeAttribute('value');
      else mirror.setAttribute('value', element.value);
      mirror.toggleAttribute('checked', element.checked);
    } else if (element instanceof HTMLTextAreaElement) {
      mirror.textContent = element.value;
    } else if (element instanceof HTMLOptionElement) {
      mirror.toggleAttribute('selected', element.selected);
    } else if (element instanceof HTMLImageElement && element.currentSrc) {
      mirror.setAttribute('src', element.currentSrc);
      if (element.currentSrc.startsWith('blob:') && !known.has(element.currentSrc)
        && element.complete && element.naturalWidth > 0) {
        blobImages.add(element.currentSrc);
      }
    } else if (element instanceof HTMLStyleElement && element.sheet) {
      // Capture CSSOM edits made after the original style element was inserted.
      // 捕获样式元素插入之后通过 CSSOM 做出的变更。
      mirror.textContent = Array.from(element.sheet.cssRules, rule => rule.cssText).join('\n').replace(/<\/style/gi, '\\3c /style');
    }
    if (!(element instanceof HTMLTextAreaElement) && !(element instanceof HTMLStyleElement && element.sheet)) {
      projectChildren(element, mirror);
    }
    if (element.shadowRoot) {
      const shadow = element.shadowRoot;
      const template = document.createElement('template');
      template.setAttribute('data-vulcan-shadow', shadow.delegatesFocus ? 'delegates-focus' : '');
      for (const hovered of Array.from(shadow.querySelectorAll(':hover'))) hover.add(hovered);
      projectChildren(shadow, template.content);
      appendAdopted(shadow, template.content);
      mirror.appendChild(template);
    }
  }

  // Populate a detached tree while omitting source templates and active content.
  // 生成离线树，同时略过源页面模板和可执行内容。
  function projectChildren(source: Node, destination: Node): void {
    for (const child of Array.from(source.childNodes)) {
      if (child instanceof Element) {
        if (child.matches('script,noscript,template,iframe,object,embed,meta,base')) continue;
        const mirror = child.cloneNode(false) as Element;
        projectElement(child, mirror);
        destination.appendChild(mirror);
      } else if (child.nodeType === Node.TEXT_NODE) {
        destination.appendChild(child.cloneNode(false));
      }
    }
  }
  projectElement(root, copy);
  const head = copy.querySelector('head');
  if (head) appendAdopted(document, head);
  // Remove executable and embedded content before sending markup across the Chromium boundary.
  // 在标记跨越 Chromium 边界前移除可执行及嵌入内容。
  const html = copy.outerHTML;
  const windowScroll = [scrollX, scrollY];
  const { innerWidth: width, innerHeight: height } = window;
  const base = document.baseURI;
  // Blob URLs are readable only in the source renderer. Read existing image blobs with a shared byte budget.
  // Blob 地址只能在源渲染器读取。用共享字节预算读取已经存在的图片 Blob。
  const blobs: { url: string; mimeType: string; dataBase64: string }[] = [];
  let remaining = blobLimit;
  for (const url of blobImages) {
    if (remaining <= 0) break;
    try {
      const response = await fetch(url);
      const length = Number(response.headers.get('content-length'));
      if (!response.ok || length > remaining) continue;
      const blob = await response.blob();
      if (!blob.size || blob.size > remaining) continue;
      const dataBase64 = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => {
          const result = reader.result as string;
          resolve(result.slice(result.indexOf(',') + 1));
        };
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(blob);
      });
      remaining -= blob.size;
      blobs.push({ url, mimeType: blob.type, dataBase64 });
    } catch { /* The page can revoke an object URL between the DOM read and the Blob read.
               页面可能在读取 DOM 与读取 Blob 之间撤销对象 URL。 */ }
  }
  return { documentId: nodes.id, base, html, width, height, scroll, windowScroll, focused, passwords, blobs };
}

// Normalize an HTTP resource URL while preserving the fragment used by the viewer.
// 标准化 HTTP 资源 URL，同时保留查看器使用的片段。
function resourceUrl(value: string, base?: string): URL | null {
  try {
    const url = new URL(value, base);
    return (url.protocol === 'http:' || url.protocol === 'https:') && !url.username && !url.password ? url : null;
  } catch { return null; }
}

// Rewrite CSS URL tokens, including imports written with quoted strings.
// 改写 CSS 的 URL 词元，包括用引号字符串编写的导入。
function rewriteCss(text: string, resolve: (url: string) => string, context = 'stylesheet'): string {
  try {
    const ast = parseCss(text, { context, parseCustomProperty: true });
    walk(ast, (node, item) => {
      if (node.type === 'Atrule' && node.name.toLowerCase() === 'import' && node.prelude?.type === 'AtrulePrelude') {
        const source = node.prelude.children.first;
        if (source?.type === 'String') source.value = resolve(source.value);
      }
      if (node.type === 'Function' && /^(?:-webkit-)?image-set$/i.test(node.name)) {
        node.children.forEach(child => { if (child.type === 'String') child.value = resolve(child.value); });
      }
      if (node.type === 'Url') node.value = resolve(node.value);
      if (node.type === 'PseudoClassSelector' && node.name.toLowerCase() === 'hover') {
        // Attribute selectors preserve the specificity of :hover.
        // 属性选择器保留 :hover 的优先级。
        item.data = { type: 'AttributeSelector', name: { type: 'Identifier', name: 'data-vulcan-hover' },
          matcher: null, value: null, flags: null };
      }
    });
    return generate(ast);
  } catch { return ''; }
}

// Accept only image, font and stylesheet bodies matching the Chromium resource type.
// 只接受与 Chromium 资源类型匹配的图片、字体和样式表正文。
function resourceKind(type: string, mimeType: string): string | null {
  if (type === 'Image' && IMAGE_TYPE.test(mimeType)) return 'image';
  if (type === 'Font' && FONT_TYPE.test(mimeType)) return 'font';
  if (type === 'Stylesheet' && mimeType === 'text/css') return 'stylesheet';
  return null;
}

// Encode the smallest contiguous markup replacement between two accepted revisions.
// 编码两个已接受修订之间最小的连续标记替换。
function markupPatch(before: string, after: string) {
  let start = 0;
  let end = before.length;
  let nextEnd = after.length;
  while (start < end && start < nextEnd && before[start] === after[start]) start++;
  while (end > start && nextEnd > start && before[end - 1] === after[nextEnd - 1]) {
    end--;
    nextEnd--;
  }
  // Keep paired surrogate units together when the diff crosses an astral character.
  // 差异跨越辅助平面字符时保持代理对完整。
  if (start > 0 && /[\uD800-\uDBFF]/.test(before.charAt(start - 1))) start--;
  if (nextEnd < after.length && /[\uDC00-\uDFFF]/.test(after.charAt(nextEnd))) {
    end++;
    nextEnd++;
  }
  return { start, end, text: after.slice(start, nextEnd) };
}

// Capture resources only from original Chromium responses. Each tab owns a separate URL index.
// 只从原始 Chromium 响应捕获资源。每个标签页拥有独立的 URL 索引。
export class DomProjection {
  private readonly pages = new WeakMap<Page, PageAssets>();
  private readonly assets = new Map<string, Asset>();
  private readonly pending = new WeakMap<Page, Promise<Awaited<ReturnType<typeof readDocument>>>>();
  private bytes = 0;
  private sequence = 0;

  // Remember real stylesheet redirect destinations while a new page is being registered.
  // 新页面登记期间记录样式表的真实重定向终点。
  observeContext(context: BrowserContext): void {
    context.on('response', response => {
      const request = response.request();
      if (request.resourceType() !== 'stylesheet' || !request.redirectedFrom() || !response.ok()) return;
      // Worker requests are not associated with a frame; only the projected top frame is eligible.
      // Worker 请求没有所属框架，仅接收投影顶层框架的响应。
      if (request.serviceWorker()) return;
      const frame = request.frame();
      const page = frame.page();
      if (page.isClosed() || frame !== page.mainFrame()) return;
      const redirects = this.pageAssets(page).initialStylesheetRedirects;
      if (!redirects) return;
      for (let alias = request.redirectedFrom(); alias && redirects.size < ASSET_COUNT_LIMIT; alias = alias.redirectedFrom()) {
        redirects.set(alias.url(), response.url());
      }
    });
  }

  // Observe one Chromium target using loader IDs. Only a committed top-level document changes the active index.
  // 使用加载器标识观察一个 Chromium 目标。只有顶层文档提交才切换活动索引。
  async trackPage(page: Page, cdp: CDPSession): Promise<void> {
    const state = this.pageAssets(page);
    const loaders = new Map<string, Map<string, CapturedAsset>>();
    const pending = new Map<string, ResourceRequest>();
    let rootFrame = '';

    // Keep the active loader plus a bounded number of provisional or recent loaders.
    // 保留活动加载器和有限数量的暂存或近期加载器。
    function resources(loader: string): Map<string, CapturedAsset> {
      const existing = loaders.get(loader);
      if (existing) return existing;
      const index = new Map<string, CapturedAsset>();
      loaders.set(loader, index);
      if (loaders.size > REVISION_HISTORY_LIMIT) {
        for (const key of loaders.keys()) if (key !== state.loader && key !== loader) {
          loaders.delete(key);
          break;
        }
      }
      return index;
    }

    // Bind the resource index and invalidate in-flight snapshots when Chromium commits a document.
    // 在 Chromium 提交文档时绑定资源索引，并使在途快照失效。
    function commit(frame: { id: string; loaderId: string }) {
      rootFrame = frame.id;
      if (state.loader === frame.loaderId) return;
      state.loader = frame.loaderId;
      state.requests = resources(frame.loaderId);
      state.published.clear();
    }

    cdp.on('Page.frameNavigated', ({ frame }) => {
      if (!frame.parentId) commit(frame);
    });
    cdp.on('Network.requestWillBeSent', event => {
      if (event.frameId !== rootFrame || !event.loaderId) return;
      if (event.type !== 'Image' && event.type !== 'Font' && event.type !== 'Stylesheet') return;
      const previous = pending.get(event.requestId);
      const urls = previous && event.redirectResponse ? previous.urls : [];
      if (urls.length >= ASSET_COUNT_LIMIT) return;
      urls.push(event.request.url);
      pending.set(event.requestId, { loader: event.loaderId, urls, kind: event.type, sequence: ++this.sequence });
      if (pending.size > ASSET_COUNT_LIMIT) {
        const oldest = pending.keys().next();
        if (!oldest.done) pending.delete(oldest.value);
      }
    });
    cdp.on('Network.responseReceived', event => {
      const request = pending.get(event.requestId);
      if (!request || request.loader !== event.loaderId
        || !(event.response.status >= 200 && event.response.status < 300 || event.response.status === 304)) return;
      const mimeType = event.response.mimeType.split(';', 1)[0]?.trim().toLowerCase() ?? '';
      if (!resourceKind(event.type, mimeType)) return;
      request.kind = event.type;
      request.mimeType = mimeType;
    });
    cdp.on('Network.loadingFailed', ({ requestId }) => { pending.delete(requestId); });
    cdp.on('Network.loadingFinished', ({ requestId, encodedDataLength }) => {
      const request = pending.get(requestId);
      pending.delete(requestId);
      if (!request?.mimeType || encodedDataLength > ASSET_LIMIT) return;
      const mimeType = request.mimeType;
      const kind = resourceKind(request.kind, mimeType);
      const index = resources(request.loader);
      const url = request.urls.at(-1);
      if (!kind || !url) return;
      void cdp.send('Network.getResponseBody', { requestId }).then(({ body, base64Encoded }) => {
        if (!page.isClosed() && loaders.get(request.loader) === index) {
          this.storeResponse(index, request.urls, url, kind, mimeType, request.sequence,
            Buffer.from(body, base64Encoded ? 'base64' : 'utf8'));
        }
      }).catch(() => { /* Chromium can release an individual response body.
                       Chromium 可能释放单个响应正文。 */ });
    });
    await cdp.send('Page.enable');
    const { frameTree } = await cdp.send('Page.getFrameTree');
    if (!rootFrame) commit(frameTree.frame);
    await cdp.send('Network.enable', { maxTotalBufferSize: STORE_LIMIT, maxResourceBufferSize: ASSET_LIMIT });

    // Read cached resources immediately, then pick up requests that were still in flight at attachment.
    // 立即读取已缓存的资源，之后再补采附加时仍在途的请求。
    const initialLoader = state.loader;
    await this.seedInitialResources(page, cdp, state, initialLoader);
    // Pre-attachment styles may finish while the parser waits for them and before a slow image completes.
    // 附加前开始的样式可能在解析器等待期间完成，早于慢图片加载完成。
    void (async () => {
      await page.waitForLoadState('domcontentloaded');
      await this.seedInitialResources(page, cdp, state, initialLoader);
      await page.waitForLoadState('load');
      await this.seedInitialResources(page, cdp, state, initialLoader);
    })()
      .catch(() => {
        // The initial document may close or time out while waiting for its remaining resources.
        // 首个文档可能在等待剩余资源期间关闭或超时。
      }).finally(() => { state.initialStylesheetRedirects = null; });
  }

  // Seed the initial committed loader from Chromium's cache without making new requests.
  // 从 Chromium 缓存补采初次提交的加载器资源，不发出新的网络请求。
  private async seedInitialResources(page: Page, cdp: CDPSession, state: PageAssets, initialLoader: string): Promise<void> {
    if (state.loader !== initialLoader || page.isClosed()) return;
    const tree = await cdp.send('Page.getResourceTree').catch(() => null);
    if (!tree) return;
    const loaded = tree.frameTree;
    if (loaded.frame.loaderId !== initialLoader || state.loader !== initialLoader) return;
    const index = state.requests;
    // Read cached bodies in sequence to bound peak CDP response memory on resource-heavy pages.
    // 依次读取缓存正文，限制资源密集页面的 CDP 响应峰值内存。
    for (const resource of loaded.resources.slice(0, ASSET_COUNT_LIMIT)) {
      if (state.loader !== initialLoader || page.isClosed()) break;
      const kind = resourceKind(resource.type, resource.mimeType);
      if (!kind || resource.failed || resource.canceled
        || (resource.contentSize !== undefined && resource.contentSize > ASSET_LIMIT) || index.has(resource.url)) continue;
      try {
        const data = await cdp.send('Page.getResourceContent', { frameId: loaded.frame.id, url: resource.url });
        if (state.loader === initialLoader && !page.isClosed()) {
          // The resource tree uses the request alias; resolve CSS relative URLs against the final response.
          // 资源树使用请求别名；CSS 相对地址以最终响应为基准解析。
          const base = state.initialStylesheetRedirects?.get(resource.url) ?? resource.url;
          this.storeResponse(index, [resource.url, base], base, kind, resource.mimeType, 0,
            Buffer.from(data.content, data.base64Encoded ? 'base64' : 'utf8'));
        }
      } catch { /* An in-flight resource may not be readable until the follow-up scan.
                 在途资源可能需要等后续扫描才能读取。 */ }
    }
  }

  // Index captured bytes by redirect aliases without replacing a newer response.
  // 按重定向别名登记已捕获字节，保留更新的响应。
  private storeResponse(index: Map<string, CapturedAsset>, urls: string[], base: string, kind: string,
    mimeType: string, sequence: number, body: Buffer): void {
    if (!body.length || body.length > ASSET_LIMIT) return;
    const id = this.put({ mimeType, body, dependencies: [] });
    for (const alias of urls) {
      const url = resourceUrl(alias);
      if (!url) continue;
      url.hash = '';
      const earlier = index.get(url.href);
      if (earlier && earlier.sequence > sequence) continue;
      index.delete(url.href);
      index.set(url.href, { id, kind, url: base, sequence });
    }
    while (index.size > ASSET_COUNT_LIMIT) {
      const oldest = index.keys().next();
      if (oldest.done) break;
      index.delete(oldest.value);
    }
  }

  // Get the tab's current resource index, creating it at the first observed event.
  // 取得标签页当前的资源索引，在首次观察到事件时建立索引。
  private pageAssets(page: Page): PageAssets {
    let state = this.pages.get(page);
    if (!state) {
      state = { requests: new Map(), loader: '', published: new Set(), markup: new Map(),
        documentId: null, blobs: new Map(), initialStylesheetRedirects: new Map() };
      this.pages.set(page, state);
    }
    return state;
  }

  // Store exact bytes by MIME type and hash, with a bounded least-recently-used cache.
  // 按 MIME 类型和哈希保存确切字节，并限制最近使用缓存的大小。
  private put(asset: Asset): string {
    const id = createHash('sha256').update(asset.mimeType).update('\0').update(asset.body).digest('hex');
    if (this.assets.delete(id)) this.bytes -= asset.body.length;
    this.assets.set(id, asset);
    this.bytes += asset.body.length;
    while (this.assets.size > ASSET_COUNT_LIMIT || this.bytes > STORE_LIMIT) {
      const oldest = this.assets.entries().next();
      if (oldest.done) break;
      this.assets.delete(oldest.value[0]);
      this.bytes -= oldest.value[1].body.length;
    }
    return id;
  }

  // Project the current DOM; only one renderer evaluation per page can wait on Chromium.
  // 投影当前 DOM，每个页面最多只有一个等待 Chromium 的渲染器求值。
  async snapshot(page: Page, revision: unknown) {
    const state = this.pageAssets(page);
    const loader = state.loader;
    let operation = this.pending.get(page);
    if (!operation) {
      for (const [url, id] of state.blobs) if (!this.assets.has(id)) state.blobs.delete(url);
      operation = page.evaluate(readDocument, { documentId: randomUUID(), blobLimit: ASSET_LIMIT,
        knownBlobs: { documentId: state.documentId, urls: [...state.blobs.keys()] } });
      this.pending.set(page, operation);
      void operation.then(() => this.pending.delete(page), () => this.pending.delete(page));
    }
    let timeout: ReturnType<typeof setTimeout> | undefined;
    const deadline = new Promise<null>(resolve => { timeout = setTimeout(resolve, 250, null); });
    const source = await Promise.race([operation, deadline]).finally(() => clearTimeout(timeout));
    if (!source || state.loader !== loader || page.isClosed()) return null;
    if (state.documentId !== source.documentId) {
      state.markup.clear();
      state.blobs.clear();
      state.documentId = source.documentId;
    }
    const { html, base, blobs, ...geometry } = source;
    for (const blob of blobs) {
      if (!IMAGE_TYPE.test(blob.mimeType)) continue;
      const body = Buffer.from(blob.dataBase64, 'base64');
      if (body.length && body.length <= ASSET_LIMIT) {
        state.blobs.set(blob.url, this.put({ mimeType: blob.mimeType, body, dependencies: [] }));
      }
    }
    while (state.blobs.size > ASSET_COUNT_LIMIT) {
      const oldest = state.blobs.keys().next();
      if (oldest.done) break;
      state.blobs.delete(oldest.value);
    }
    const output = this.project(state, html, base);
    const current = { ...geometry, ...output };
    const encoded = JSON.stringify(current);
    if (Buffer.byteLength(encoded) > MARKUP_LIMIT) throw new Error('Browser DOM projection exceeds the markup limit.');
    const nextRevision = createHash('sha256').update(encoded).digest('hex');
    // Retain resource authorization for in-flight reads of earlier revisions of this document.
    // 保留同一文档早期修订正在读取的资源授权。
    for (const id of output.assets) state.published.add(id);
    for (const id of state.published) if (!this.assets.has(id)) state.published.delete(id);
    if (revision === nextRevision) return { revision: nextRevision };
    const previous = typeof revision === 'string' ? state.markup.get(revision) : undefined;
    state.markup.set(nextRevision, output.html);
    if (state.markup.size > REVISION_HISTORY_LIMIT) {
      const oldest = state.markup.keys().next();
      if (!oldest.done) state.markup.delete(oldest.value);
    }
    if (previous !== undefined) {
      const patch = markupPatch(previous, output.html);
      if (Buffer.byteLength(JSON.stringify(patch)) < Buffer.byteLength(JSON.stringify(output.html))) {
        return { revision: nextRevision, ...geometry, assets: output.assets, patch };
      }
    }
    return { revision: nextRevision, ...current };
  }

  // Return only a bounded chunk of an asset previously published for the exact tab.
  // 只返回曾向指定标签页发布的资源的有限分块。
  asset(page: Page, id: unknown, offset: unknown) {
    if (typeof id !== 'string' || !this.pageAssets(page).published.has(id)
      || typeof offset !== 'number' || !Number.isSafeInteger(offset) || offset < 0) return null;
    const asset = this.assets.get(id);
    if (!asset || offset >= asset.body.length) return null;
    const end = Math.min(offset + CHUNK_SIZE, asset.body.length);
    return { mimeType: asset.mimeType, dependencies: asset.dependencies,
      dataBase64: asset.body.subarray(offset, end).toString('base64'), nextOffset: end < asset.body.length ? end : null };
  }

  // Sanitize markup and convert resource URLs to content-addressed references.
  // 清理标记，并将资源 URL 转换为按内容寻址的引用。
  private project(state: PageAssets, html: string, base: string) {
    const dependencies = new Set<string>();
    const styles = new Map<string, string | null>();
    // Resolve captured dependencies recursively and cut stylesheet import cycles.
    // 递归解析已捕获的依赖，并截断样式表导入环。
    const resolve = (value: string, origin: string, used: Set<string>, depth: number, stylesheet = false): string => {
      if (value.startsWith('#') || /^data:(?:image|font)\//i.test(value)) return value;
      if (value.startsWith('blob:')) {
        const blob = state.blobs.get(value);
        if (!blob || stylesheet || !this.assets.has(blob)) return 'data:,';
        used.add(blob);
        return `vulcan-resource:${blob}`;
      }
      const url = resourceUrl(value, origin);
      if (!url || depth > 32) return 'data:,';
      const fragment = url.hash;
      url.hash = '';
      const record = state.requests.get(url.href);
      if (!record || (stylesheet && record.kind !== 'stylesheet')) return 'data:,';
      const asset = this.assets.get(record.id);
      if (!asset) return 'data:,';
      let id = record.id;
      if (record.kind === 'stylesheet') {
        const key = `${id}\0${record.url}`;
        const known = styles.get(key);
        if (known === null) return 'data:,';
        if (known) id = known;
        else {
          styles.set(key, null);
          const nested = new Set<string>();
          const css = rewriteCss(asset.body.toString('utf8'), value => resolve(value, record.url, nested, depth + 1));
          if (!css) return 'data:,';
          id = this.put({ mimeType: 'text/css', body: Buffer.from(css), dependencies: [...nested] });
          styles.set(key, id);
        }
      }
      used.add(id);
      return `vulcan-resource:${id}${fragment}`;
    };
    // Resolve a top-level markup resource relative to the source document's base.
    // 相对于源文档的基准地址解析顶层标记资源。
    const local = (value: string, stylesheet = false) => resolve(value, base, dependencies, 0, stylesheet);
    const document = parseHtml(html, { scriptingEnabled: true });
    const pending: DefaultTreeAdapterTypes.ParentNode[] = [document];
    for (let parent = pending.pop(); parent; parent = pending.pop()) {
      parent.childNodes = parent.childNodes.filter(node => {
        if (!('tagName' in node)) return node.nodeName === '#text';
        const tag = node.tagName.toLowerCase();
        if (tag === 'template' && 'content' in node && node.attrs.some(attr => attr.name === 'data-vulcan-shadow')) {
          pending.push(node.content as DefaultTreeAdapterTypes.DocumentFragment);
          return true;
        }
        if (REMOVE_TAGS.has(tag) || (tag === 'link' && !node.attrs.some(attr =>
          attr.name === 'rel' && attr.value.toLowerCase() === 'stylesheet'))) return false;
        node.attrs = node.attrs.filter(attr => !attr.name.startsWith('on') && !STRIP_ATTRS.has(attr.name));
        for (const attr of node.attrs) {
          if (attr.name === 'style') attr.value = rewriteCss(attr.value, local, 'declarationList');
          else if (PRESENTATION_ATTRS.has(attr.name) && /url\(/i.test(attr.value)) {
            attr.value = rewriteCss(attr.value, local, 'value');
          } else if (attr.name === 'href') {
            if (tag === 'link') attr.value = local(attr.value, true);
            else if (tag === 'image' || tag === 'feimage' || tag === 'use') attr.value = local(attr.value);
            else attr.value = '#';
          } else if (['src', 'poster', 'background'].includes(attr.name)) attr.value = local(attr.value);
        }
        if (tag === 'style') {
          for (const text of node.childNodes) if ('value' in text) {
            text.value = rewriteCss(text.value, local).replace(/<\/style/gi, '\\3c /style');
          }
        } else pending.push(node);
        return true;
      });
    }
    // Include the entire dependency graph so each dependency is authorized by the same snapshot.
    // 纳入完整的依赖图，以便同一快照授权读取每项依赖。
    for (const id of dependencies) {
      const asset = this.assets.get(id);
      if (asset) for (const child of asset.dependencies) dependencies.add(child);
    }
    return { html: serialize(document), assets: [...dependencies] };
  }
}

// Replay a text edit on the focused projected field, using an explicit range when supplied.
// 在投影中拥有焦点的字段上回放文本编辑，若请求指定范围则使用该范围。
export async function applyDomTextEdit(
  page: Page, kind: 'textEdit' | 'textCommit', params: Record<string, unknown>,
): Promise<boolean> {
  const { id, documentId, before, start, end, text } = params;
  if (typeof id !== 'string' || typeof documentId !== 'string' || typeof text !== 'string') return false;
  // Password and rich text commits use Chromium's selection without sending any previous content.
  // 密码和富文本提交使用 Chromium 选区，避免传输已有内容。
  let range: EditRange | null = null;
  if (kind === 'textEdit') {
    if (typeof before !== 'string' || typeof start !== 'number' || typeof end !== 'number'
      || !Number.isSafeInteger(start) || !Number.isSafeInteger(end)
      || start < 0 || end < start || end > before.length) return false;
    range = { before, start, end };
  } else if (!text) return false;
  const valid = await page.evaluate(({ id, documentId, range }) => {
    const nodes = (window as ProjectionWindow).__vulcanProjectionNodes;
    let field = document.activeElement;
    while (field?.shadowRoot?.activeElement) field = field.shadowRoot.activeElement;
    if (!nodes || nodes.id !== documentId || !field || nodes.ids.get(field) !== id) return false;
    if (!(field instanceof HTMLTextAreaElement || field instanceof HTMLInputElement)) {
      return range === null && field instanceof HTMLElement && field.isContentEditable;
    }
    if (field.readOnly || field.disabled
      || (field.selectionStart === null && field.type !== 'email' && field.type !== 'number')) return false;
    if (range) {
      if (field.type === 'password' || field.selectionStart === null || field.value !== range.before) return false;
      field.setSelectionRange(range.start, range.end);
    }
    return true;
  }, { id, documentId, range });
  if (!valid) return false;
  if (text) await page.keyboard.insertText(text);
  else if (range && range.start < range.end) await page.keyboard.press('Backspace');
  return true;
}
