/** Observe the main document title and report changed title/URL pairs through the registered binding. */
/** 观察主文档标题，通过已登记绑定报告变化后的标题与地址；无参数、无返回值。 */
export function observePageTitle(): void {
  if (window !== window.top) return;
  // The binding only publishes untrusted display text; it grants no host operations.
  // 绑定仅发布不可信展示文本，不授予宿主操作能力。
  const titleWindow = window as unknown as {
    __vulcanReportPageTitle: (title: string, url: string) => Promise<void>;
    __vulcanTitleObserver?: MutationObserver;
  };
  // Initial registration and new-document initialization may overlap; retain a single observer.
  // 首次登记与新文档初始化可能重叠；每个文档仅保留一个观察器。
  titleWindow.__vulcanTitleObserver?.disconnect();
  let previousTitle: string | null = null;
  let previousUrl: string | null = null;
  /** Publish only a changed pair; navigation may destroy the binding's reply before it arrives. */
  /** 仅发布变化的数据对；导航可能在绑定答复到达前销毁其上下文；无参数、无返回值。 */
  const publish = (): void => {
    const title = document.title;
    const url = location.href;
    if (title === previousTitle && url === previousUrl) return;
    previousTitle = title;
    previousUrl = url;
    // An abandoned document cannot consume its host reply; the next document reports independently.
    // 已离开的文档无法接收宿主答复；下一份文档会独立报告。
    void titleWindow.__vulcanReportPageTitle(title, url).catch(() => undefined);
  };
  // Observe document mutations to also handle a title or head node being added/replaced by an SPA.
  // 观察文档变更，同时支持 SPA 新增或替换 title、head 节点。
  const observer = new MutationObserver(publish);
  observer.observe(document, { childList: true, subtree: true, characterData: true });
  titleWindow.__vulcanTitleObserver = observer;
  publish();
}
