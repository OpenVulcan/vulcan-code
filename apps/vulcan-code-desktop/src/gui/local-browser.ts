// Client-owned temporary pages are isolated from both the Core browser and the workbench preload.
// 客户端持有的临时页面同时与 Core 浏览器及工作台预加载桥隔离。
import { randomUUID } from 'node:crypto';
import { BrowserWindow, WebContentsView, session } from 'electron';

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
  // loading reports the current native navigation state.
  // loading 报告当前原生导航状态。
  loading: boolean;
}

// LocalBrowserSnapshot names the selected GUI-local tab and its sibling tabs.
// LocalBrowserSnapshot 标识选中的 GUI 本地标签及其同级标签。
export interface LocalBrowserSnapshot {
  // activeId is null when no temporary page remains.
  // activeId 在没有临时页面时为 null。
  activeId: string | null;
  // tabs are ordered by creation and contain no Core browser sessions.
  // tabs 按创建顺序排列，且不包含 Core 浏览器会话。
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
}

// LocalBrowserEntry owns one native view and its current user-facing metadata.
// LocalBrowserEntry 持有一个原生视图及其当前面向用户的元数据。
interface LocalBrowserEntry {
  // view has no preload, Node access, or Core automation connection.
  // view 没有预加载、Node 权限或 Core 自动化连接。
  view: WebContentsView;
  // tab is the metadata emitted to the trusted workbench main frame.
  // tab 是发往受信任工作台主框架的元数据。
  tab: LocalBrowserTab;
}

// LocalBrowserManager owns temporary tabs for exactly one desktop GUI window.
// LocalBrowserManager 为一个确定的桌面 GUI 窗口持有临时标签。
export class LocalBrowserManager {
  // tabs preserves creation order and prevents a renderer from reaching native views directly.
  // tabs 保持创建顺序，并阻止渲染器直接接触原生视图。
  private readonly tabs = new Map<string, LocalBrowserEntry>();
  // activeId selects the one native page that may be visible in the sidebar.
  // activeId 选择唯一可在侧栏显示的原生页面。
  private activeId: string | null = null;
  // viewport is cleared when the sidebar unmounts or a modal covers it.
  // viewport 在侧栏卸载或模态框覆盖时清除。
  private viewport: LocalBrowserViewport | null = null;
  // partition ensures all temporary tabs share only their own in-memory browser context.
  // partition 使所有临时标签仅共享各自的内存浏览器上下文。
  private readonly partition = `vulcan-local-browser-${randomUUID()}`;

  // Constructor blocks the workbench and exact Core origins inside untrusted temporary pages.
  // 构造函数阻止不受信任的临时页面访问工作台及精确的 Core 来源。
  // window owns the native content view; blockedOrigins are the workbench and Core HTTP origins.
  // window 持有原生内容视图；blockedOrigins 是工作台和 Core 的 HTTP 来源。
  constructor(
    private readonly window: BrowserWindow,
    private readonly blockedOrigins: ReadonlySet<string>,
    private readonly onChanged: (snapshot: LocalBrowserSnapshot) => void,
  ) {
    const browserSession = session.fromPartition(this.partition);
    browserSession.setPermissionRequestHandler((_contents, _permission, callback) => callback(false));
    browserSession.setPermissionCheckHandler(() => false);
    browserSession.on('will-download', (event) => event.preventDefault());
  }

  // snapshot returns immutable, serializable tab metadata for the renderer.
  // snapshot 返回不可变、可序列化的标签元数据供渲染器使用。
  // Returns the active id and tab list without any WebContents references.
  // 返回活动标识及标签列表，不包含任何 WebContents 引用。
  snapshot(): LocalBrowserSnapshot {
    return { activeId: this.activeId, tabs: [...this.tabs.values()].map(({ tab }) => ({ ...tab })) };
  }

  // open creates a sandboxed tab and selects it after validating one HTTP destination.
  // open 校验一个 HTTP 目标后创建沙箱标签并选中。
  // url is a user-entered or popup target; returns the new tab's opaque GUI-local id.
  // url 是用户输入或弹窗目标；返回新标签在 GUI 内的不透明标识。
  open(url: string): string {
    const destination = this.normalizeUrl(url);
    if (this.tabs.size >= MAX_LOCAL_BROWSER_TABS) {
      throw new Error('The local browser has reached its tab limit.');
    }
    const id = randomUUID();
    const view = new WebContentsView({
      webPreferences: {
        partition: this.partition,
        nodeIntegration: false,
        contextIsolation: true,
        sandbox: true,
        webSecurity: true,
      },
    });
    const tab: LocalBrowserTab = { id, title: destination, url: destination, loading: true };
    const entry: LocalBrowserEntry = { view, tab };
    this.tabs.set(id, entry);
    this.window.contentView.addChildView(view);
    view.setVisible(false);
    view.webContents.setWindowOpenHandler(({ url: popupUrl }) => {
      try {
        this.open(popupUrl);
      } catch {
        // Non-HTTP and internal workbench popups have no temporary-browser destination.
        // 非 HTTP 或内部工作台弹窗在临时浏览器中没有可用目标。
      }
      return { action: 'deny' };
    });
    view.webContents.on('will-navigate', (event, target) => {
      if (!this.isAllowedUrl(target)) event.preventDefault();
    });
    view.webContents.on('will-redirect', (event, target) => {
      if (!this.isAllowedUrl(target)) event.preventDefault();
    });
    view.webContents.on('did-navigate', (_event, currentUrl) => this.update(id, { url: currentUrl }));
    view.webContents.on('did-navigate-in-page', (_event, currentUrl) => this.update(id, { url: currentUrl }));
    view.webContents.on('page-title-updated', (_event, title) => this.update(id, { title }));
    view.webContents.on('did-start-loading', () => this.update(id, { loading: true }));
    view.webContents.on('did-stop-loading', () => this.update(id, { loading: false }));
    this.select(id);
    void view.webContents.loadURL(destination).catch(() => this.update(id, { loading: false }));
    return id;
  }

  // navigate loads one exact existing tab without changing any Core browser page.
  // navigate 加载一个精确的现有标签，不改变任何 Core 浏览器页面。
  // id selects a GUI-local tab; url is the new HTTP destination.
  // id 选择 GUI 本地标签；url 是新的 HTTP 目标。
  navigate(id: string, url: string): void {
    const entry = this.requireTab(id);
    const destination = this.normalizeUrl(url);
    void entry.view.webContents.loadURL(destination).catch(() => this.update(id, { loading: false }));
  }

  // select makes one existing tab the only potentially visible native view.
  // select 使一个现有标签成为唯一可能显示的原生视图。
  // id is the GUI-local tab to select.
  // id 是要选中的 GUI 本地标签。
  select(id: string): void {
    this.requireTab(id);
    this.activeId = id;
    this.layout();
    this.emit();
  }

  // close destroys one temporary page and selects a surviving sibling when available.
  // close 销毁一个临时页面，并在有同级页面时选中其一。
  // id is the exact GUI-local tab to release.
  // id 是要释放的精确 GUI 本地标签。
  close(id: string): void {
    const entry = this.requireTab(id);
    this.window.contentView.removeChildView(entry.view);
    entry.view.webContents.close();
    this.tabs.delete(id);
    if (this.activeId === id) this.activeId = this.tabs.keys().next().value ?? null;
    this.layout();
    this.emit();
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
    }
    this.viewport = viewport;
    this.layout();
  }

  // dispose releases every GUI-local page before the owning window closes.
  // dispose 在所属窗口关闭前释放全部 GUI 本地页面。
  dispose(): void {
    for (const id of [...this.tabs.keys()]) this.close(id);
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

  // normalizeUrl accepts only HTTP(S) or the inert initial blank page outside internal origins.
  // normalizeUrl 仅接受 HTTP(S) 或内部来源之外的惰性初始空白页面。
  // input is the requested address; returns its canonical navigable URL.
  // input 是请求地址；返回其规范化的可导航 URL。
  private normalizeUrl(input: string): string {
    const value = input.trim();
    const parsed = new URL(value === 'about:blank' || /^[a-z][a-z\d+.-]*:/i.test(value)
      ? value : `https://${value}`);
    if (!this.isAllowedUrl(parsed.href)) throw new Error('This address is unavailable in the local browser.');
    return parsed.href;
  }

  // isAllowedUrl prevents untrusted pages from reaching the privileged workbench or Core origin.
  // isAllowedUrl 阻止不受信任页面到达有权限的工作台或 Core 来源。
  // target is a top-level URL; returns whether native navigation may continue.
  // target 是顶层 URL；返回原生导航是否可以继续。
  private isAllowedUrl(target: string): boolean {
    try {
      const parsed = new URL(target);
      return parsed.href === 'about:blank'
        || (['http:', 'https:'].includes(parsed.protocol) && !this.blockedOrigins.has(parsed.origin));
    } catch {
      return false;
    }
  }

  // update publishes changed page metadata only while its tab still exists.
  // update 仅在标签仍存在时发布变化的页面元数据。
  // id selects the exact tab; patch contains native navigation facts.
  // id 选择精确标签；patch 包含原生导航事实。
  private update(id: string, patch: Partial<LocalBrowserTab>): void {
    const entry = this.tabs.get(id);
    if (!entry) return;
    Object.assign(entry.tab, patch);
    this.emit();
  }

  // layout hides every inactive page and clips the selected page to the visible sidebar.
  // layout 隐藏所有非活动页面，并将选中页面裁剪到可见侧栏范围。
  private layout(): void {
    const content = this.window.getContentBounds();
    const viewport = this.viewport;
    for (const [id, { view }] of this.tabs) {
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
      view.setBounds({ x: left, y: top, width: right - left, height: bottom - top });
      view.setVisible(true);
    }
  }

  // emit reports a fresh tab snapshot to the owning workbench main frame.
  // emit 向所属工作台主框架报告新的标签快照。
  private emit(): void {
    this.onChanged(this.snapshot());
  }
}
