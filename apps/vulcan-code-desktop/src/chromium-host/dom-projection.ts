import { createHash, randomUUID } from 'node:crypto';
import { parse as parseHtml, serialize, type DefaultTreeAdapterTypes } from 'parse5';
import { generate, parse as parseCss, walk } from 'css-tree';
import type { BrowserContext, Page, Request } from 'playwright-core';

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
type Edit = { id: string; documentId: string; before: string; start: number; end: number; text: string };
type Asset = { mimeType: string; body: Buffer; dependencies: string[] };
type CapturedAsset = { id: string; kind: string; url: string; sequence: number };
type PageAssets = { requests: Map<string, CapturedAsset>; generation: number; published: Set<string>;
  markup: Map<string, string>; documentId: string | null; blobs: Map<string, string> };
type SourceNodes = { id: string; ids: WeakMap<Element, string>; next: number };

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
  const copy = root.cloneNode(true) as Element;
  const originals = [root, ...Array.from(root.querySelectorAll('*'))];
  const copies = [copy, ...Array.from(copy.querySelectorAll('*'))];
  const scroll: [string, number, number][] = [];
  const blobImages = new Set<string>();
  const known = new Set(knownBlobs.documentId === nodes.id ? knownBlobs.urls : []);
  let focused: string | null = null;
  for (const [index, element] of originals.entries()) {
    // The deep clone has the same element order as the original tree.
    // 深度克隆保持原始树的元素顺序。
    const mirror = copies[index]!;
    let id = nodes.ids.get(element);
    if (!id) {
      id = String(++nodes.next);
      nodes.ids.set(element, id);
    }
    mirror.setAttribute('data-vulcan-node', id);
    if (element.scrollLeft || element.scrollTop) scroll.push([id, element.scrollLeft, element.scrollTop]);
    if (element === document.activeElement) focused = id;
    if (element instanceof HTMLInputElement) {
      if (element.type === 'password' || element.type === 'file') mirror.removeAttribute('value');
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
      mirror.textContent = Array.from(element.sheet.cssRules, rule => rule.cssText).join('\n');
    }
  }
  // Remove executable and embedded content before sending markup across the Chromium boundary.
  // 在标记跨越 Chromium 边界前移除可执行及嵌入内容。
  for (const element of Array.from(copy.querySelectorAll('script,noscript,template,iframe,object,embed,meta,base'))) element.remove();
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
  return { documentId: nodes.id, base, html, width, height, scroll, windowScroll, focused, blobs };
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
    walk(ast, node => {
      if (node.type === 'Atrule' && node.name.toLowerCase() === 'import' && node.prelude?.type === 'AtrulePrelude') {
        const source = node.prelude.children.first;
        if (source?.type === 'String') source.value = resolve(source.value);
      }
      if (node.type === 'Function' && /^(?:-webkit-)?image-set$/i.test(node.name)) {
        node.children.forEach(child => { if (child.type === 'String') child.value = resolve(child.value); });
      }
      if (node.type === 'Url') node.value = resolve(node.value);
    });
    return generate(ast);
  } catch { return ''; }
}

// Read the top-level owning page of a network request, including early popup requests.
// 获取网络请求所属的顶层页面，包括弹窗发出的早期请求。
function requestPage(request: Request): Page | null {
  try {
    const frame = request.frame();
    const page = frame.page();
    return frame === page.mainFrame() ? page : null;
  } catch { return null; }
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
  private readonly requestGenerations = new WeakMap<Request, number>();
  private bytes = 0;
  private sequence = 0;

  // Subscribe before navigation so the first response of each page can be captured.
  // 在导航之前订阅，以便捕获每个页面的首批响应。
  constructor(context: BrowserContext) {
    context.on('request', request => {
      const page = requestPage(request);
      if (!page) return;
      const state = this.pageAssets(page);
      if (request.isNavigationRequest() && !request.redirectedFrom()) {
        state.generation++;
        state.requests.clear();
        state.published.clear();
      }
      this.requestGenerations.set(request, state.generation);
    });
    context.on('response', response => {
      const request = response.request();
      const kind = request.resourceType();
      const mimeType = response.headers()['content-type']?.split(';')[0]?.trim().toLowerCase() ?? '';
      if (!response.ok() || !(kind === 'image' && IMAGE_TYPE.test(mimeType)
        || kind === 'font' && FONT_TYPE.test(mimeType) || kind === 'stylesheet' && mimeType === 'text/css')) return;
      if (Number(response.headers()['content-length']) > ASSET_LIMIT) return;
      const page = requestPage(request);
      if (!page) return;
      const state = this.pageAssets(page);
      const generation = this.requestGenerations.get(request);
      const sequence = ++this.sequence;
      void response.body().then(body => {
        if (page.isClosed() || generation !== state.generation || !body.length || body.length > ASSET_LIMIT) return;
        const id = this.put({ mimeType, body, dependencies: [] });
        for (let current: Request | null = request; current; current = current.redirectedFrom()) {
          const url = resourceUrl(current.url());
          if (!url) continue;
          url.hash = '';
          const earlier = state.requests.get(url.href);
          if (earlier && earlier.sequence > sequence) continue;
          state.requests.delete(url.href);
          state.requests.set(url.href, { id, kind, url: response.url(), sequence });
        }
        while (state.requests.size > ASSET_COUNT_LIMIT) {
          const oldest = state.requests.keys().next();
          if (oldest.done) break;
          state.requests.delete(oldest.value);
        }
      }).catch(() => { /* Chromium may discard a response body after navigation.
                       导航后 Chromium 可能释放响应正文。 */ });
    });
  }

  // Get the tab's current resource index, creating it at the first observed event.
  // 取得标签页当前的资源索引，在首次观察到事件时建立索引。
  private pageAssets(page: Page): PageAssets {
    let state = this.pages.get(page);
    if (!state) {
      state = { requests: new Map(), generation: 0, published: new Set(), markup: new Map(),
        documentId: null, blobs: new Map() };
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
    const generation = state.generation;
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
    if (!source || state.generation !== generation || page.isClosed()) return null;
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

// Verify the exact document, focused field and UTF-16 range before replaying one committed edit.
// 在回放一次已提交的编辑前，验证确切文档、焦点字段和 UTF-16 范围。
export async function applyDomTextEdit(page: Page, params: Record<string, unknown>): Promise<boolean> {
  const { id, documentId, before, start, end, text } = params;
  if (typeof id !== 'string' || typeof documentId !== 'string' || typeof before !== 'string' || typeof text !== 'string'
    || typeof start !== 'number' || typeof end !== 'number' || !Number.isSafeInteger(start) || !Number.isSafeInteger(end)
    || start < 0 || end < start || end > before.length) return false;
  const valid = await page.evaluate(({ id, documentId, before, start, end }: Edit) => {
    const nodes = (window as ProjectionWindow).__vulcanProjectionNodes;
    const field = document.activeElement;
    if (!nodes || nodes.id !== documentId || !field || nodes.ids.get(field) !== id
      || !(field instanceof HTMLTextAreaElement || field instanceof HTMLInputElement)
      || field.selectionStart === null || field.readOnly || field.disabled || field.value !== before) return false;
    field.setSelectionRange(start, end);
    return true;
  }, { id, documentId, before, start, end, text });
  if (!valid) return false;
  if (text) await page.keyboard.insertText(text);
  else if (start < end) await page.keyboard.press('Backspace');
  return true;
}
