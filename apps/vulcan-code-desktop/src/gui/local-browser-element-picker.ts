// Lets the user point at one element of a temporary page and returns it with an annotated screenshot.
// 让用户在临时页面中指向一个元素，并返回该元素及带标注的截图。
import type { WebContents } from 'electron';

// PICKER_WORLD_ID isolates the picker's script from the page's own JavaScript world.
// PICKER_WORLD_ID 将选择器脚本与页面自身的 JavaScript 世界隔离。
const PICKER_WORLD_ID = 1207;

// PICKER_TEXT_LIMIT bounds the element text excerpt handed to the composer.
// PICKER_TEXT_LIMIT 限制交给输入区的元素文本摘录长度。
const PICKER_TEXT_LIMIT = 300;

// LocalBrowserElementPick is one element the user chose, in the page's CSS pixels.
// LocalBrowserElementPick 是用户选中的一个元素，以页面 CSS 像素表示。
export interface LocalBrowserElementPick {
  // url and title identify the page at the moment of the pick.
  // url 与 title 标识选中时的页面。
  url: string;
  title: string;
  // selector is a CSS selector that matched exactly this element when it was picked.
  // selector 是选中时恰好匹配此元素的 CSS 选择器。
  selector: string;
  // tag is the lower-case element name.
  // tag 是小写的元素名。
  tag: string;
  // text is a whitespace-collapsed excerpt of the element's visible text.
  // text 是元素可见文本经空白折叠后的摘录。
  text: string;
  // rect is the element's border box relative to the page viewport.
  // rect 是元素边框盒相对于页面视口的位置。
  rect: { x: number; y: number; width: number; height: number };
  // viewport is the page viewport size the rect is measured against.
  // viewport 是度量 rect 所依据的页面视口尺寸。
  viewport: { width: number; height: number };
  // screenshot is a PNG of the visible page with the picked element outlined.
  // screenshot 是可见页面的 PNG，已标出选中的元素。
  screenshot: Uint8Array;
}

// PICKER_SCRIPT highlights the element under the pointer, resolves with the clicked one while leaving its
// outline in place for the screenshot, and resolves null on Escape or cancellation.
// PICKER_SCRIPT 高亮指针下的元素，点击后返回该元素并保留其轮廓供截图使用，按 Escape 或取消时返回 null。
const PICKER_SCRIPT = `new Promise((resolve) => {
  globalThis.__vulcanPickCancel?.();
  const box = document.createElement('div');
  box.style.cssText = 'position:fixed;z-index:2147483647;pointer-events:none;box-sizing:border-box;'
    + 'border:2px solid #f26b1d;background:rgba(242,107,29,0.12);display:none;';
  document.documentElement.appendChild(box);
  globalThis.__vulcanPickBox = box;
  let current = null;
  const place = (element) => {
    const r = element.getBoundingClientRect();
    Object.assign(box.style, { display: 'block', left: r.left + 'px', top: r.top + 'px',
      width: r.width + 'px', height: r.height + 'px' });
  };
  const selectorOf = (element) => {
    const parts = [];
    for (let node = element; node && node.nodeType === 1 && node !== document.documentElement; node = node.parentElement) {
      if (node.id) {
        const byId = '#' + CSS.escape(node.id);
        if (document.querySelectorAll(byId).length === 1) { parts.unshift(byId); break; }
      }
      const tag = node.tagName.toLowerCase();
      const siblings = node.parentElement
        ? [...node.parentElement.children].filter((child) => child.tagName === node.tagName) : [];
      parts.unshift(siblings.length > 1 ? tag + ':nth-of-type(' + (siblings.indexOf(node) + 1) + ')' : tag);
    }
    return parts.join(' > ');
  };
  const stop = (event) => { event.preventDefault(); event.stopPropagation(); event.stopImmediatePropagation(); };
  const move = (event) => {
    const element = document.elementFromPoint(event.clientX, event.clientY);
    if (element && element !== box) { current = element; place(element); }
  };
  const finish = (value) => {
    removeEventListener('mousemove', move, true);
    removeEventListener('mousedown', stop, true);
    removeEventListener('mouseup', stop, true);
    removeEventListener('click', click, true);
    removeEventListener('keydown', key, true);
    globalThis.__vulcanPickCancel = undefined;
    if (value === null) { box.remove(); resolve(null); return; }
    box.style.background = 'transparent';
    requestAnimationFrame(() => requestAnimationFrame(() => resolve(value)));
  };
  const click = (event) => {
    stop(event);
    const element = current ?? document.elementFromPoint(event.clientX, event.clientY);
    if (!element || element === box) { finish(null); return; }
    place(element);
    const r = element.getBoundingClientRect();
    finish({
      selector: selectorOf(element),
      tag: element.tagName.toLowerCase(),
      text: (element.innerText || element.textContent || '').replace(/\\s+/g, ' ').trim().slice(0, ${PICKER_TEXT_LIMIT}),
      rect: { x: r.left, y: r.top, width: r.width, height: r.height },
      viewport: { width: innerWidth, height: innerHeight },
    });
  };
  const key = (event) => { if (event.key === 'Escape') { stop(event); finish(null); } };
  addEventListener('mousemove', move, true);
  addEventListener('mousedown', stop, true);
  addEventListener('mouseup', stop, true);
  addEventListener('click', click, true);
  addEventListener('keydown', key, true);
  globalThis.__vulcanPickCancel = () => finish(null);
})`;

// CANCEL_SCRIPT ends a running pick; CLEANUP_SCRIPT removes the outline left for the screenshot.
// CANCEL_SCRIPT 结束进行中的选择；CLEANUP_SCRIPT 移除为截图保留的轮廓。
const CANCEL_SCRIPT = 'globalThis.__vulcanPickCancel?.(); undefined';
const CLEANUP_SCRIPT = 'globalThis.__vulcanPickBox?.remove(); globalThis.__vulcanPickBox = undefined; undefined';

// PickedFacts is the part of a pick produced inside the page before it is validated.
// PickedFacts 是页面内产生、尚未校验的选择结果部分。
type PickedFacts = Omit<LocalBrowserElementPick, 'url' | 'title' | 'screenshot'>;

// pickLocalBrowserElement runs one pick on `contents`; resolves null when the user cancels or the page
// navigates away first.
// pickLocalBrowserElement 在 `contents` 上执行一次选择；用户取消或页面先行跳转时返回 null。
export async function pickLocalBrowserElement(contents: WebContents): Promise<LocalBrowserElementPick | null> {
  contents.focus();
  // abandoned resolves when the page leaves, because its isolated world and pending promise go with it.
  // abandoned 在页面离开时完成，因为其隔离世界及待定 Promise 会随之消失。
  let release = () => {};
  const abandoned = new Promise<null>((resolve) => {
    const leave = (details: { isMainFrame: boolean; isSameDocument: boolean }) => {
      if (details.isMainFrame && !details.isSameDocument) resolve(null);
    };
    const gone = () => resolve(null);
    contents.on('did-start-navigation', leave);
    contents.once('destroyed', gone);
    release = () => {
      contents.off('did-start-navigation', leave);
      contents.off('destroyed', gone);
    };
  });
  try {
    const picked: unknown = await Promise.race([
      contents.executeJavaScriptInIsolatedWorld(PICKER_WORLD_ID, [{ code: PICKER_SCRIPT }], true),
      abandoned,
    ]);
    const facts = validatePickedFacts(picked);
    if (!facts) return null;
    try {
      const image = await contents.capturePage();
      return {
        ...facts,
        url: contents.getURL(),
        title: contents.getTitle(),
        screenshot: new Uint8Array(image.toPNG()),
      };
    } finally {
      await contents.executeJavaScriptInIsolatedWorld(PICKER_WORLD_ID, [{ code: CLEANUP_SCRIPT }]).catch(() => undefined);
    }
  } finally {
    release();
  }
}

// cancelLocalBrowserElementPick ends a running pick on `contents`; nothing happens when none is running.
// cancelLocalBrowserElementPick 结束 `contents` 上进行中的选择；没有进行中的选择时不产生任何效果。
export async function cancelLocalBrowserElementPick(contents: WebContents): Promise<void> {
  await contents.executeJavaScriptInIsolatedWorld(PICKER_WORLD_ID, [{ code: CANCEL_SCRIPT }]);
}

// validatePickedFacts accepts only the exact shape the picker script produces.
// validatePickedFacts 只接受选择器脚本产生的精确结构。
function validatePickedFacts(value: unknown): PickedFacts | null {
  if (value === null || typeof value !== 'object') return null;
  const facts = value as Record<string, unknown>;
  const rect = facts.rect as Record<string, unknown> | undefined;
  const viewport = facts.viewport as Record<string, unknown> | undefined;
  const finite = (input: unknown): input is number => typeof input === 'number' && Number.isFinite(input);
  if (typeof facts.selector !== 'string' || typeof facts.tag !== 'string' || typeof facts.text !== 'string'
    || !rect || !viewport || ![rect.x, rect.y, rect.width, rect.height, viewport.width, viewport.height].every(finite)) {
    return null;
  }
  return {
    selector: facts.selector,
    tag: facts.tag,
    text: facts.text.slice(0, PICKER_TEXT_LIMIT),
    rect: { x: rect.x as number, y: rect.y as number, width: rect.width as number, height: rect.height as number },
    viewport: { width: viewport.width as number, height: viewport.height as number },
  };
}
