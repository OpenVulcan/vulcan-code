// Client-owned temporary pages are isolated from both the Core browser and the workbench preload.
// 客户端持有的临时页面同时与 Core 浏览器及工作台预加载桥隔离。
import { randomUUID } from 'node:crypto';
import { BrowserWindow, WebContentsView, session, shell } from 'electron';
import { isAllowedLocalBrowserUrl, normalizeLocalBrowserUrl, validateExternalBrowserUrl } from './local-browser-url.js';

// Bounds native views created by user actions and untrusted page popups in one GUI window.
// 限制单个 GUI 窗口由用户操作或不受信任页面弹窗创建的原生视图数量。
const MAX_LOCAL_BROWSER_TABS = 16;

// LocalBrowserTab is safe metadata displayed by the workbench, never an automation identity.
// LocalBrowserTab 是供工作台显示的安全元数据，绝不是自动化身份。
export interface LocalBrowserTab {
  // id identifies one GUI-local page until that page closes.
  // id 在页面关闭前标识一个 GUI 本地页面。
  id: string;
  // title comes from the untrusted page and is rendered as text only.
  // title 来自不受信任的页面，只能作为文本渲染。
  title: string;
  // url is the current top-level address of this temporary page.
  // url 是此临时页面当前的顶层地址。
  url: string;
  // faviconUrl is a native-reported HTTP(S) image URL, or null when unavailable.
  // faviconUrl 是原生事件报告的 HTTP(S) 图标地址，不可用时为空。
  faviconUrl: string | null;
  // loading reports the current native navigation state.
  // loading 报告当前原生导航状态。
  loading: boolean;
  // Native history determines whether navigation controls are available.
  // 原生历史决定导航控件是否可用。
  canGoBack: boolean;
  canGoForward: boolean;
}

// LocalBrowserSnapshot names the selected GUI-local tab and its sibling tabs.
// LocalBrowserSnapshot 标识选中的 GUI 本地标签及其同级标签。
export interface LocalBrowserSnapshot {
  // ownerSessionId identifies the conversation represented by this window-local snapshot.
  // ownerSessionId 标识此窗口本地快照所代表的会话。
  ownerSessionId: string | null;
  // activeId is null when no temporary page remains.
  // activeId 在没有临时页面时为 null。
  activeId: string | null;
  // tabs follow the user's visual order and contain no Core browser sessions.
  // tabs 按用户的视觉顺序排列，且不包含 Core 浏览器会话。
  tabs: LocalBrowserTab[];
}

// LocalBrowserViewport is measured in renderer CSS pixels before native scaling.
// LocalBrowserViewport 在原生缩放前以渲染器 CSS 像素度量。
export interface LocalBrowserViewport {
  // x and y are the placeholder's visible top-left coordinates.
  // x 与 y 是占位区域可见的左上角坐标。
  x: number;
  y: number;
  // width and height are its visible dimensions.
  // width 与 height 是其可见尺寸。
  width: number;
  height: number;
  // viewportWidth and viewportHeight map CSS pixels onto Electron content bounds.
  // viewportWidth 与 viewportHeight 将 CSS 像素映射到 Electron 内容边界。
  viewportWidth: number;
  viewportHeight: number;
  // visible is false while the panel is obscured or a workbench modal is open.
  // visible 在面板被遮挡或工作台模态框打开时为 false。
  visible: boolean;
  // An omitted or null deviceSize uses the panel size; otherwise fit this CSS viewport inside it.
  // 未提供或为空的 deviceSize 使用面板尺寸；否则在面板内适配此 CSS 视口。
  deviceSize?: { width: number; height: number } | null;
}

// LocalBrowserEntry owns one native view and its current user-facing metadata.
// LocalBrowserEntry 持有一个原生视图及其当前面向用户的元数据。
interface LocalBrowserEntry {
  // ownerSessionId binds this native page to one conversation in this GUI window.
  // ownerSessionId 将此原生页面绑定到该 GUI 窗口中的一个会话。
  ownerSessionId: string;
  // view has no preload, Node access, or Core automation connection.
  // view 没有预加载、Node 权限或 Core 自动化连接。
  view: WebContentsView;
  // tab is the metadata emitted to the trusted workbench main frame.
  // tab 是发往受信任工作台主框架的元数据。
  tab: LocalBrowserTab;
  // emulationKey avoids resetting Chromium emulation on unchanged layout notifications.
  // emulationKey 避免在布局未变化的通知中重设 Chromium 模拟。
  emulationKey: string;
  // documentReady is set only by main-frame dom-ready, after Chromium creates its render view.
  // documentReady 仅由主框架 dom-ready 设置，此时 Chromium 已创建渲染视图。
  documentReady: boolean;
}

// LocalBrowserManager owns temporary tabs for exactly one desktop GUI window.
// LocalBrowserManager 为一个确定的桌面 GUI 窗口持有临时标签。
export class LocalBrowserManager {
  // tabs preserves user ordering and prevents a renderer from reaching native views directly.
  // tabs 保持用户排序，并阻止渲染器直接接触原生视图。
  private readonly tabs = new Map<string, LocalBrowserEntry>();
  // activeId selects the one native page that may be visible in the sidebar.
  // activeId 选择唯一可在侧栏显示的原生页面。
  private activeId: string | null = null;
  // currentOwnerSessionId is the conversation whose local tabs are visible in this window.
  // currentOwnerSessionId 是此窗口当前可见本地标签所属的会话。
  private currentOwnerSessionId: string | null = null;
  // activeIdByOwner remembers each conversation's last selected local tab.
  // activeIdByOwner 记住每个会话最后选中的本地标签。
  private readonly activeIdByOwner = new Map<string, string>();
  // viewport is cleared when the sidebar unmounts or a modal covers it.
  // viewport 在侧栏卸载或模态框覆盖时清除。
  private viewport: LocalBrowserViewport | null = null;
  // ownerPartitions shares in-memory website state only among tabs of the same conversation.
  // ownerPartitions 仅让同一会话的标签共享内存中的网页状态。
  private readonly ownerPartitions = new Map<string, string>();

  // Constructor blocks the workbench and exact Core origins inside untrusted temporary pages.
  // 构造函数阻止不受信任的临时页面访问工作台及精确的 Core 来源。
  // window owns the native content view; blockedOrigins are the workbench and Core HTTP origins.
  // window 持有原生内容视图；blockedOrigins 是工作台和 Core 的 HTTP 来源。
  constructor(
    private readonly window: BrowserWindow,
    private readonly blockedOrigins: ReadonlySet<string>,
    private readonly onChanged: (snapshot: LocalBrowserSnapshot) => void,
  ) {
    // Native cleanup starts before a full reload, even if the renderer cannot send its cleanup message.
    // 完整重载开始前由原生端清理，即使渲染界面无法发送清理消息也能执行。
    window.webContents.on('did-start-navigation', (details) => {
      if (details.isMainFrame && !details.isSameDocument) this.dispose();
    });
    window.webContents.on('render-process-gone', () => this.dispose());
  }

  // initialize acknowledges a new renderer by releasing every previous native page.
  // initialize 通过释放全部旧原生页面，确认新渲染界面的初始化请求。
  // Takes no arguments and returns after resetting state and publishing an empty snapshot.
  // 无参数；重置状态并发布空快照后返回。
  initialize(): void {
    this.dispose();
    this.emit();
  }

  // partitionForOwner creates one sandboxed, non-persistent browser session per conversation.
  // partitionForOwner 为每个会话创建独立、沙箱化且不落盘的浏览器会话。
  // ownerSessionId identifies the conversation; returns its window-local Electron partition.
  // ownerSessionId 标识会话；返回该窗口内专属的 Electron 分区。
  private partitionForOwner(ownerSessionId: string): string {
    const existing = this.ownerPartitions.get(ownerSessionId);
    if (existing) return existing;
    const partition = `vulcan-local-browser-${randomUUID()}`;
    const browserSession = session.fromPartition(partition);
    browserSession.setPermissionRequestHandler((_contents, _permission, callback) => callback(false));
    browserSession.setPermissionCheckHandler(() => false);
    browserSession.on('will-download', (event) => event.preventDefault());
    this.ownerPartitions.set(ownerSessionId, partition);
    return partition;
  }

  // snapshot returns immutable, serializable tab metadata for the renderer.
  // snapshot 返回不可变、可序列化的标签元数据供渲染器使用。
  // Returns the active id and tab list without any WebContents references.
  // 返回活动标识及标签列表，不包含任何 WebContents 引用。
  snapshot(): LocalBrowserSnapshot {
    return { ownerSessionId: this.currentOwnerSessionId, activeId: this.activeId, tabs: [...this.tabs.values()]
      .filter((entry) => entry.ownerSessionId === this.currentOwnerSessionId)
      .map(({ tab }) => ({ ...tab })) };
  }

  // setOwner changes the displayed conversation without moving or destroying its native tabs.
  // setOwner 切换显示的会话，不迁移也不销毁其原生标签。
  // ownerSessionId is a persisted conversation id or null; returns only that owner's tabs.
  // ownerSessionId 是持久会话标识或空值；仅返回该归属的标签。
  setOwner(ownerSessionId: string | null): LocalBrowserSnapshot {
    if (ownerSessionId !== null && (typeof ownerSessionId !== 'string' || ownerSessionId.trim() !== ownerSessionId || !ownerSessionId)) {
      throw new Error('Invalid local browser conversation owner.');
    }
    this.currentOwnerSessionId = ownerSessionId;
    const remembered = ownerSessionId ? this.activeIdByOwner.get(ownerSessionId) : null;
    this.activeId = remembered && this.tabs.get(remembered)?.ownerSessionId === ownerSessionId
      ? remembered : [...this.tabs.entries()].find(([, entry]) => entry.ownerSessionId === ownerSessionId)?.[0] ?? null;
    this.viewport = null;
    this.layout();
    this.emit();
    return this.snapshot();
  }

  // open creates a sandboxed tab and selects it after validating one HTTP destination.
  // open 校验一个 HTTP 目标后创建沙箱标签并选中。
  // url is a user-entered or popup target; returns the new tab's opaque GUI-local id.
  // url 是用户输入或弹窗目标；返回新标签在 GUI 内的不透明标识。
  open(url: string): string {
    if (!this.currentOwnerSessionId) throw new Error('Select a conversation before opening a local browser tab.');
    return this.openForOwner(this.currentOwnerSessionId, url);
  }

  // openForOwner preserves the originating conversation when an untrusted page opens a popup.
  // openForOwner 在不受信任页面打开弹窗时保留发起页面的会话归属。
  // ownerSessionId is the fixed creator; url is the requested destination; returns one local id.
  // ownerSessionId 是固定创建者；url 是目标地址；返回一个本地标识。
  private openForOwner(ownerSessionId: string, url: string): string {
    const destination = this.normalizeUrl(url);
    if (this.tabs.size >= MAX_LOCAL_BROWSER_TABS) {
      throw new Error('The local browser has reached its tab limit.');
    }
    const id = randomUUID();
    const view = new WebContentsView({
      webPreferences: {
        partition: this.partitionForOwner(ownerSessionId),
        nodeIntegration: false,
        contextIsolation: true,
        sandbox: true,
        webSecurity: true,
      },
    });
    const tab: LocalBrowserTab = { id, title: destination, url: destination, faviconUrl: null, loading: true,
      canGoBack: false, canGoForward: false };
    const entry: LocalBrowserEntry = { ownerSessionId, view, tab, emulationKey: '', documentReady: false };
    this.tabs.set(id, entry);
    this.window.contentView.addChildView(view);
    view.setVisible(false);
    view.webContents.setWindowOpenHandler(({ url: popupUrl }) => {
      try {
        this.openForOwner(ownerSessionId, popupUrl);
      } catch {
        // Unsupported schemes and internal workbench popups have no temporary-browser destination.
        // 不支持的协议或内部工作台弹窗在临时浏览器中没有可用目标。
      }
      return { action: 'deny' };
    });
    view.webContents.on('will-navigate', (event, target) => {
      if (!this.isAllowedUrl(target)) event.preventDefault();
    });
    view.webContents.on('will-redirect', (event, target) => {
      if (!this.isAllowedUrl(target)) event.preventDefault();
    });
    view.webContents.on('did-navigate', (_event, currentUrl) => {
      // A committed navigation may replace the render view; wait for the new document's dom-ready.
      // 已提交导航可能替换渲染视图；等待新文档的 dom-ready。
      entry.documentReady = false;
      entry.emulationKey = '';
      this.update(id, { url: currentUrl, faviconUrl: null });
    });
    view.webContents.on('dom-ready', () => {
      entry.documentReady = true;
      this.layout();
    });
    view.webContents.on('render-process-gone', () => {
      entry.documentReady = false;
      entry.emulationKey = '';
    });
    view.webContents.on('did-navigate-in-page', (_event, currentUrl, isMainFrame) => {
      if (isMainFrame) this.update(id, { url: currentUrl });
    });
    view.webContents.on('page-title-updated', (_event, title) => this.update(id, { title }));
    // Accept only native-reported web images outside the protected workbench origins.
    // 仅接受原生报告的网页图标，并排除受保护的工作台来源。
    view.webContents.on('page-favicon-updated', (_event, favicons) => {
      this.update(id, { faviconUrl: favicons.find((url) => /^https?:\/\//i.test(url) && this.isAllowedUrl(url)) ?? null });
    });
    view.webContents.on('did-start-loading', () => this.update(id, { loading: true }));
    view.webContents.on('did-stop-loading', () => this.update(id, { loading: false }));
    if (ownerSessionId === this.currentOwnerSessionId) this.select(id);
    void view.webContents.loadURL(destination).catch(() => this.update(id, { loading: false }));
    return id;
  }

  // navigate loads one exact existing tab without changing any Core browser page.
  // navigate 加载一个精确的现有标签，不改变任何 Core 浏览器页面。
  // id selects a GUI-local tab; url is the new web address or absolute local file path.
  // id 选择 GUI 本地标签；url 是新的网页地址或本地文件绝对路径。
  navigate(id: string, url: string): void {
    const entry = this.requireCurrentTab(id);
    const destination = this.normalizeUrl(url);
    void entry.view.webContents.loadURL(destination).catch(() => this.update(id, { loading: false }));
  }

  // history acts on the selected owner's exact tab using Chromium's real history.
  // history 使用 Chromium 真实历史操作当前归属的精确标签。
  // id identifies the page; action selects back, forward, reload or stop; returns nothing.
  // id 标识页面；action 选择后退、前进、刷新或停止；无返回值。
  history(id: string, action: 'back' | 'forward' | 'reload' | 'stop'): void {
    // contents owns navigation state, including redirects and same-document entries.
    // contents 持有导航状态，包括重定向和同文档历史条目。
    const contents = this.requireCurrentTab(id).view.webContents;
    switch (action) {
      case 'back': if (contents.navigationHistory.canGoBack()) contents.navigationHistory.goBack(); break;
      case 'forward': if (contents.navigationHistory.canGoForward()) contents.navigationHistory.goForward(); break;
      case 'reload': contents.reload(); break;
      case 'stop': contents.stop(); break;
      default: throw new Error('Unknown local browser navigation action.');
    }
  }

  // move places one tab at a sibling's position without recreating native pages or changing selection.
  // move 将标签移动到同级标签的位置，不重建原生页面也不改变选中状态。
  // id is the dragged tab and targetId is the destination sibling; returns nothing.
  // id 是拖动标签，targetId 是目标同级标签；无返回值。
  move(id: string, targetId: string): void {
    this.requireCurrentTab(id);
    this.requireCurrentTab(targetId);
    if (id === targetId) return;
    // ordered preserves every other owner's relative order as well as page identities.
    // ordered 同时保留其他归属的相对顺序及页面身份。
    const ordered = [...this.tabs.entries()];
    // destination is measured before removal so a forward drag passes the target tab.
    // destination 在移除前计算，使向后拖动能经过目标标签。
    const destination = ordered.findIndex(([key]) => key === targetId);
    // moved retains the original WebContentsView and its full navigation state.
    // moved 保留原始 WebContentsView 及完整导航状态。
    const [moved] = ordered.splice(ordered.findIndex(([key]) => key === id), 1);
    ordered.splice(destination, 0, moved!);
    this.tabs.clear();
    for (const [key, entry] of ordered) this.tabs.set(key, entry);
    this.emit();
  }

  // openDeveloperTools opens the requested page inspector in a separate native window.
  // openDeveloperTools 在独立原生窗口打开请求页面的检查器。
  // id belongs to the current conversation; returns nothing after opening DevTools.
  // id 属于当前会话；打开开发者工具后无返回值。
  openDeveloperTools(id: string): void {
    this.requireCurrentTab(id).view.webContents.openDevTools({ mode: 'detach', activate: true });
  }

  // openExternal opens the actual committed website or file address externally.
  // openExternal 在外部打开实际已提交的网页或文件地址。
  // id identifies the current owner's page; resolves when the OS accepts the request.
  // id 标识当前归属的页面；操作系统接受请求后完成。
  async openExternal(id: string): Promise<void> {
    // Read the committed native URL rather than a renderer-supplied address for this tab.
    // 读取此标签原生页面的已提交地址，而不是渲染器提供的地址。
    await this.openExternalUrl(this.requireCurrentTab(id).view.webContents.getURL());
  }

  // openExternalUrl opens one explicit conversation link through the OS without creating a native tab.
  // openExternalUrl 通过操作系统打开一个明确的会话链接，不创建原生标签。
  // url is an HTTP(S) or file URL; resolves after dispatch, rejecting unsupported schemes and protected origins.
  // url 是 HTTP(S) 或 file URL；派发后完成，不支持的协议和受保护来源会被拒绝。
  async openExternalUrl(url: string): Promise<void> {
    await shell.openExternal(validateExternalBrowserUrl(url, this.blockedOrigins));
  }

  // select makes one existing tab the only potentially visible native view.
  // select 使一个现有标签成为唯一可能显示的原生视图。
  // id is the GUI-local tab to select.
  // id 是要选中的 GUI 本地标签。
  select(id: string): void {
    this.requireCurrentTab(id);
    this.activeId = id;
    this.activeIdByOwner.set(this.currentOwnerSessionId!, id);
    this.layout();
    this.emit();
  }

  // close destroys one temporary page and selects a surviving sibling when available.
  // close 销毁一个临时页面，并在有同级页面时选中其一。
  // id is the exact GUI-local tab to release.
  // id 是要释放的精确 GUI 本地标签。
  close(id: string): void {
    const entry = this.requireCurrentTab(id);
    this.window.contentView.removeChildView(entry.view);
    entry.view.webContents.close();
    this.tabs.delete(id);
    if (this.activeId === id) {
      this.activeId = [...this.tabs.entries()].find(([, candidate]) => candidate.ownerSessionId === this.currentOwnerSessionId)?.[0] ?? null;
      if (this.activeId) this.activeIdByOwner.set(this.currentOwnerSessionId!, this.activeId);
      else this.activeIdByOwner.delete(this.currentOwnerSessionId!);
    }
    this.layout();
    this.emit();
  }

  // closeOwner confirms before releasing all pages of one conversation, including inactive pages.
  // closeOwner 确认后释放一个会话的全部页面，包括非活动页面。
  // ownerSessionId identifies the conversation; confirm receives its page count; returns false on cancellation.
  // ownerSessionId 标识会话；confirm 接收页面数量；取消时返回 false。
  // A true result waits for actual WebContents destruction before the renderer removes its browser panel.
  // 返回 true 前等待 WebContents 实际销毁，渲染界面随后才移除浏览器面板。
  async closeOwner(ownerSessionId: string, confirm: (count: number) => Promise<boolean>): Promise<boolean> {
    if (typeof ownerSessionId !== 'string' || !ownerSessionId.trim()) throw new Error('Invalid local browser conversation owner.');
    // count covers loaded pages and blank native tabs, both of which consume resources.
    // count 包含已加载页面和空白原生标签，两者都会占用资源。
    const count = [...this.tabs.values()].filter((entry) => entry.ownerSessionId === ownerSessionId).length;
    if (count > 0 && !await confirm(count)) return false;
    // closing is refreshed after confirmation to include pages created while the dialog was open.
    // closing 在确认后重新取得，包含弹窗期间创建的页面。
    const closing = [...this.tabs.entries()].filter(([, entry]) => entry.ownerSessionId === ownerSessionId);
    for (const [id] of closing) this.tabs.delete(id);
    this.activeIdByOwner.delete(ownerSessionId);
    this.ownerPartitions.delete(ownerSessionId);
    if (this.currentOwnerSessionId === ownerSessionId) {
      this.activeId = null;
      this.viewport = null;
    }
    await Promise.all(closing.map(([, { view }]) => new Promise<void>((resolve) => {
      view.setVisible(false);
      this.window.contentView.removeChildView(view);
      view.webContents.once('destroyed', () => resolve());
      view.webContents.close();
    })));
    if (!this.window.isDestroyed()) this.emit();
    return true;
  }

  // setViewport maps the rendered sidebar rectangle to native content coordinates.
  // setViewport 将已渲染的侧栏矩形映射到原生内容坐标。
  // viewport is null when the panel unmounts; otherwise all dimensions must be finite.
  // viewport 在面板卸载时为 null；否则所有尺寸必须为有限数值。
  setViewport(viewport: LocalBrowserViewport | null): void {
    if (viewport !== null) {
      if (typeof viewport !== 'object' || viewport === undefined) {
        throw new Error('Invalid local browser viewport.');
      }
      const values = [viewport.x, viewport.y, viewport.width, viewport.height,
        viewport.viewportWidth, viewport.viewportHeight];
      if (!values.every((value) => typeof value === 'number' && Number.isFinite(value))
        || typeof viewport.visible !== 'boolean' || viewport.viewportWidth <= 0
        || viewport.viewportHeight <= 0 || viewport.width < 0 || viewport.height < 0) {
        throw new Error('Invalid local browser viewport.');
      }
      if (viewport.deviceSize != null && (typeof viewport.deviceSize !== 'object'
        || ![viewport.deviceSize.width, viewport.deviceSize.height]
          .every((value) => Number.isInteger(value) && value >= 240 && value <= 3840))) {
        throw new Error('Browser viewport dimensions must be integers between 240 and 3840.');
      }
    }
    this.viewport = viewport;
    this.layout();
  }

  // dispose releases every GUI-local page on window close, reload or renderer loss.
  // dispose 在窗口关闭、重载或渲染进程丢失时释放全部 GUI 本地页面。
  // Takes no arguments; the empty manager remains reusable by the next renderer.
  // 无参数；清空后的管理器可由下一个渲染界面复用。
  dispose(): void {
    // retired detaches metadata first so late native events cannot revive stale tabs.
    // retired 先移除元数据，避免迟到的原生事件恢复旧标签。
    const retired = [...this.tabs.values()];
    this.tabs.clear();
    for (const { view } of retired) {
      view.setVisible(false);
      this.window.contentView.removeChildView(view);
      view.webContents.close();
    }
    this.ownerPartitions.clear();
    this.activeIdByOwner.clear();
    this.activeId = null;
    this.currentOwnerSessionId = null;
    this.viewport = null;
  }

  // requireTab rejects an unknown renderer-supplied tab id.
  // requireTab 拒绝渲染器传入的未知标签标识。
  // id is the requested tab; returns its exact native entry.
  // id 是请求的标签；返回其精确原生条目。
  private requireTab(id: string): LocalBrowserEntry {
    const entry = this.tabs.get(id);
    if (!entry) throw new Error('Unknown local browser tab.');
    return entry;
  }

  // requireCurrentTab prevents renderer actions from reaching another conversation's local tab.
  // requireCurrentTab 阻止渲染器动作访问另一会话的本地标签。
  // id selects a tab; returns it only when it belongs to the selected conversation.
  // id 选择标签；仅当其属于当前会话时返回该标签。
  private requireCurrentTab(id: string): LocalBrowserEntry {
    const entry = this.requireTab(id);
    if (entry.ownerSessionId !== this.currentOwnerSessionId) throw new Error('Local browser tab belongs to another conversation.');
    return entry;
  }

  // normalizeUrl accepts web addresses, local files and the inert blank page outside internal origins.
  // normalizeUrl 接受网页地址、本地文件和惰性空白页，并排除内部来源。
  // input is the requested address; returns its canonical navigable URL.
  // input 是请求地址；返回其规范化的可导航 URL。
  private normalizeUrl(input: string): string {
    return normalizeLocalBrowserUrl(input, this.blockedOrigins);
  }

  // isAllowedUrl prevents untrusted pages from reaching the privileged workbench or Core origin.
  // isAllowedUrl 阻止不受信任页面到达有权限的工作台或 Core 来源。
  // target is a top-level URL; returns whether native navigation may continue.
  // target 是顶层 URL；返回原生导航是否可以继续。
  private isAllowedUrl(target: string): boolean {
    return isAllowedLocalBrowserUrl(target, this.blockedOrigins);
  }

  // update publishes changed page metadata only while its tab still exists.
  // update 仅在标签仍存在时发布变化的页面元数据。
  // id selects the exact tab; patch contains native navigation facts.
  // id 选择精确标签；patch 包含原生导航事实。
  private update(id: string, patch: Partial<LocalBrowserTab>): void {
    const entry = this.tabs.get(id);
    if (!entry) return;
    Object.assign(entry.tab, patch);
    entry.tab.canGoBack = entry.view.webContents.navigationHistory.canGoBack();
    entry.tab.canGoForward = entry.view.webContents.navigationHistory.canGoForward();
    if (entry.ownerSessionId === this.currentOwnerSessionId) this.emit();
  }

  // layout hides every inactive page and clips the selected page to the visible sidebar.
  // layout 隐藏所有非活动页面，并将选中页面裁剪到可见侧栏范围。
  private layout(): void {
    const content = this.window.getContentBounds();
    const viewport = this.viewport;
    for (const [id, entry] of this.tabs) {
      // view is kept alive when tabs or display modes change.
      // view 在切换标签或显示模式时保持存活。
      const { view } = entry;
      const visible = id === this.activeId && viewport?.visible === true;
      if (!visible || !viewport) {
        view.setVisible(false);
        continue;
      }
      const scaleX = content.width / viewport.viewportWidth;
      const scaleY = content.height / viewport.viewportHeight;
      const left = Math.max(0, Math.round(viewport.x * scaleX));
      const top = Math.max(0, Math.round(viewport.y * scaleY));
      const right = Math.min(content.width, Math.round((viewport.x + viewport.width) * scaleX));
      const bottom = Math.min(content.height, Math.round((viewport.y + viewport.height) * scaleY));
      if (right <= left || bottom <= top) {
        view.setVisible(false);
        continue;
      }
      // Both modes declare their current viewport explicitly: Electron 44 restores a stale host size
      // when disabling emulation after fitting. Panel mode therefore uses current bounds at scale one.
      // 两种模式均明确声明当前视口：Electron 44 在适配后关闭模拟时会恢复陈旧宿主尺寸，
      // 因此面板模式使用当前边界和一倍缩放，不依赖旧尺寸缓存。
      const size = viewport.deviceSize ?? { width: right - left, height: bottom - top };
      // scale fits the requested viewport without changing its responsive layout width.
      // scale 适配请求视口，同时保持响应式布局宽度不变。
      const scale = Math.min(1, (right - left) / size.width, (bottom - top) / size.height);
      // width and height are the fitted native view dimensions in device-independent pixels.
      // width 和 height 是适配后的原生视图尺寸，单位为设备独立像素。
      const width = Math.max(1, Math.floor(size.width * scale));
      const height = Math.max(1, Math.floor(size.height * scale));
      view.setBounds({ x: left + Math.floor((right - left - width) / 2),
        y: top + Math.floor((bottom - top - height) / 2), width, height });
      // key prevents geometry-only movement from resetting device emulation.
      // key 防止仅位置变化时重设设备模拟。
      const key = `${size.width}:${size.height}:${scale}`;
      // Electron dereferences the native render view inside enableDeviceEmulation; calling it before
      // dom-ready can terminate the entire GUI, so only native document readiness permits this call.
      // Electron 在 enableDeviceEmulation 内部解引用原生渲染视图；dom-ready 前调用会终止整个 GUI，
      // 因此只有原生文档就绪事件才允许执行此调用。
      if (entry.documentReady && entry.emulationKey !== key) {
        view.webContents.enableDeviceEmulation({ screenPosition: 'desktop', screenSize: size,
          viewPosition: { x: 0, y: 0 }, deviceScaleFactor: 0, viewSize: size, scale });
        entry.emulationKey = key;
      }
      view.setVisible(true);
    }
  }

  // emit reports a fresh tab snapshot to the owning workbench main frame.
  // emit 向所属工作台主框架报告新的标签快照。
  private emit(): void {
    this.onChanged(this.snapshot());
  }
}
