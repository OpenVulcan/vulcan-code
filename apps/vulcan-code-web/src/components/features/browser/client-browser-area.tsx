import * as React from "react";
import { ArrowLeft, ArrowRight, Bug, ExternalLink, Globe, MonitorSmartphone, MoreHorizontal, Plus, RotateCw, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { useI18n, type Translate } from "@/i18n";
import {
  readElectronLocalBrowserBridge,
  type ElectronLocalBrowserSnapshot,
  type ElectronLocalBrowserViewport,
} from "@/host-shell/electron-bridge";

// localBrowserAddress validates web destinations for the ordinary-browser fallback without a native file loader.
// localBrowserAddress 为没有原生文件加载器的普通浏览器降级入口校验网页目标。
// input is the user's address; returns a canonical URL or throws a visible validation error.
// input 是用户地址；返回规范 URL，或抛出可见的校验错误。
export function localBrowserAddress(input: string): string {
  const value = input.trim();
  if (!value) throw new Error("Enter a website address.");
  // loopback selects HTTP for development servers without requiring a typed scheme.
  // loopback 让未输入协议的本机开发服务使用 HTTP。
  const loopback = /^(?:localhost|127(?:\.\d{1,3}){3}|\[::1\])(?::|\/|$)/i.test(value);
  // complete preserves explicit HTTP(S) addresses and exposes other schemes to validation.
  // complete 保留显式 HTTP(S) 地址，并把其他协议交给后续校验。
  const complete = /^https?:\/\//i.test(value) ? value : /^[a-z][a-z\d+.-]*:\/\//i.test(value)
    ? value : `${loopback ? "http" : "https"}://${value}`;
  const url = new URL(complete);
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("Only HTTP and HTTPS websites can be opened here.");
  }
  return url.href;
}

// localBrowserTabLabel bounds untrusted page titles without losing the full hover title.
// localBrowserTabLabel 限制不受信任页面标题的显示长度，同时保留悬停时的完整标题。
// value is the native title or URL and t resolves the blank-page label; returns a compact tab label.
// value 是原生标题或地址，t 解析空白页标签；返回紧凑的标签文案。
export function localBrowserTabLabel(value: string, t: Translate): string {
  const title = value.trim() === "about:blank" ? t("browser.newTab") : value.trim();
  const characters = Array.from(title);
  return characters.length > 24 ? `${characters.slice(0, 23).join("")}…` : title;
}

// LocalBrowserTabIcon displays a native favicon and falls back if that image fails.
// LocalBrowserTabIcon 显示原生网页图标，并在图片加载失败时使用默认图标。
// faviconUrl is an optional image source; returns a decorative fourteen-pixel icon.
// faviconUrl 是可为空的图片来源；返回十四像素装饰图标。
function LocalBrowserTabIcon({ faviconUrl }: { faviconUrl: string | null }) {
  // failed belongs to this source; the parent keys the component by its URL.
  // failed 属于当前来源；父组件使用地址作为组件键。
  const [failed, setFailed] = React.useState(false);
  return faviconUrl && !failed
    ? <img src={faviconUrl} alt="" width={14} height={14} draggable={false} referrerPolicy="no-referrer"
        onError={() => setFailed(true)} />
    : <Globe size={14} aria-hidden="true" />;
}

// ClientBrowserArea gives this client a temporary browser unrelated to Core-owned AI pages.
// ClientBrowserArea 为本客户端提供与 Core 自有 AI 页面无关的临时浏览器。
// Returns an embedded native view in Electron or a deliberate new-tab link in ordinary WebUI.
// 在 Electron 中返回内嵌原生视图，在普通 WebUI 中返回明确的新标签链接。
export function ClientBrowserArea(props: { ownerSessionId: string }) {
  // t controls only visible labels, not navigation or trust boundaries.
  // t 仅控制可见文案，不改变导航或信任边界。
  const { t } = useI18n();
  // bridge exists only in an Electron workbench with the isolated native view capability.
  // bridge 仅在具备隔离原生视图能力的 Electron 工作台存在。
  const bridge = React.useMemo(readElectronLocalBrowserBridge, []);
  // snapshot is the current GUI-local tab list, never a Core session list.
  // snapshot 是当前 GUI 本地标签列表，绝不是 Core 会话列表。
  const [snapshot, setSnapshot] = React.useState<ElectronLocalBrowserSnapshot>({ ownerSessionId: null, activeId: null, tabs: [] });
  // ownerReady allows tab actions only after the native manager switches to this conversation.
  // ownerReady 仅在原生管理器切换到当前会话后允许操作标签。
  const [ownerReady, setOwnerReady] = React.useState(false);
  // addressDraft belongs to one tab; without an actual edit the native URL always remains authoritative.
  // addressDraft 属于一个标签；未实际编辑时始终以原生地址为准。
  const [addressDraft, setAddressDraft] = React.useState<{ tabId: string | null; value: string } | null>(null);
  // freeSize enables an explicit responsive viewport fitted inside the panel.
  // freeSize 启用在面板内适配显示的明确响应式视口。
  const [freeSize, setFreeSize] = React.useState(false);
  // deviceSize retains validated CSS dimensions when switching display modes.
  // deviceSize 在切换显示模式时保留已校验的 CSS 尺寸。
  const [deviceSize, setDeviceSize] = React.useState({ width: 393, height: 852 });
  // draggedId identifies only a tab started in this component's tab strip.
  // draggedId 仅标识从当前组件标签栏开始拖动的标签。
  const draggedId = React.useRef<string | null>(null);
  // dropId highlights the destination while dragging without rebuilding native pages.
  // dropId 在拖动时突出目标位置，不重建原生页面。
  const [dropId, setDropId] = React.useState<string | null>(null);
  // error makes a refused destination or native-view failure visible.
  // error 使被拒绝的目标或原生视图故障可见。
  const [error, setError] = React.useState<string | null>(null);
  // placeholder locates the native view below the local tab and address controls.
  // placeholder 将原生视图定位于本地标签与地址控件下方。
  const placeholder = React.useRef<HTMLDivElement | null>(null);
  // addressInput preserves in-progress editing while native navigation updates the tab URL.
  // addressInput 在原生导航更新标签 URL 时保留正在进行的编辑。
  const addressInput = React.useRef<HTMLInputElement | null>(null);

  React.useEffect(() => {
    if (!bridge) return;
    let cancelled = false;
    let changeRevision = 0;
    // unsubscribe is installed only after Core has selected this window's conversation owner.
    // unsubscribe 仅在原生端选中此窗口的会话归属后安装。
    let unsubscribe = () => {};
    setOwnerReady(false);
    void bridge.setOwner(props.ownerSessionId).then(async (initial) => {
      if (cancelled) return;
      if (initial.ownerSessionId !== props.ownerSessionId) throw new Error("Local browser switched to a different conversation.");
      setSnapshot(initial);
      unsubscribe = bridge.subscribeChanged((next) => {
        changeRevision += 1;
        if (!cancelled && next.ownerSessionId === props.ownerSessionId) setSnapshot(next);
      });
      // requestedRevision stops a delayed snapshot from replacing a newer native event.
      // requestedRevision 防止迟到的快照覆盖较新的原生事件。
      const requestedRevision = changeRevision;
      // latest fills the narrow gap between owner selection and event subscription.
      // latest 补上切换归属与订阅事件之间的短暂空档。
      const latest = await bridge.snapshot();
      if (cancelled) return;
      if (latest.ownerSessionId !== props.ownerSessionId) throw new Error("Local browser switched to a different conversation.");
      if (changeRevision === requestedRevision) setSnapshot(latest);
      setOwnerReady(true);
    }).catch((reason: unknown) => {
      if (!cancelled) setError(String(reason));
    });
    return () => {
      cancelled = true;
      unsubscribe();
      void bridge.viewport(null);
    };
  }, [bridge, props.ownerSessionId]);

  // activeTab is this GUI's selected temporary page, not the selected AI automation target.
  // activeTab 是当前 GUI 选中的临时页面，不是 AI 自动化目标。
  const activeTab = snapshot.tabs.find((tab) => tab.id === snapshot.activeId) ?? null;
  // address displays a real edit or the latest committed page URL, even when the input has focus.
  // address 显示真实编辑草稿或最新已提交页面地址，即使输入框仍有焦点也能更新。
  const address = addressDraft?.tabId === snapshot.activeId ? addressDraft.value
    : activeTab && activeTab.url !== "about:blank" ? activeTab.url : "";
  React.useEffect(() => {
    setAddressDraft(null);
    draggedId.current = null;
    setDropId(null);
  }, [snapshot.activeId, props.ownerSessionId]);

  React.useEffect(() => {
    if (!bridge) return;
    let frame = 0;
    let lastSent = "";
    // update maps visible DOM geometry and modal state into the native sidebar view.
    // update 把可见 DOM 几何与模态框状态映射到侧栏原生视图。
    const update = (): void => {
      frame = 0;
      const element = placeholder.current;
      if (!element || !snapshot.activeId) {
        if (lastSent !== "null") {
          lastSent = "null";
          void bridge.viewport(null).catch((reason: unknown) => setError(String(reason)));
        }
        return;
      }
      const bounds = element.getBoundingClientRect();
      // Radix confirmation dialogs use alertdialog without necessarily emitting aria-modal.
      // Radix 确认弹窗使用 alertdialog，不保证输出 aria-modal。
      const modalOpen = document.querySelector('[role="dialog"][data-state="open"], [role="alertdialog"][data-state="open"], [aria-modal="true"], [role="menu"][data-state="open"]') !== null;
      const visible = bounds.width > 0 && bounds.height > 0 && bounds.bottom > 0 && bounds.right > 0
        && bounds.top < window.innerHeight && bounds.left < window.innerWidth
        && getComputedStyle(element).visibility !== "hidden" && !modalOpen;
      const viewport: ElectronLocalBrowserViewport = {
        x: bounds.left, y: bounds.top, width: bounds.width, height: bounds.height,
        viewportWidth: window.innerWidth, viewportHeight: window.innerHeight, visible,
        deviceSize: freeSize ? deviceSize : null,
      };
      const serialized = JSON.stringify(viewport);
      if (serialized === lastSent) return;
      lastSent = serialized;
      void bridge.viewport(viewport).catch((reason: unknown) => setError(String(reason)));
    };
    // schedule coalesces scroll, resize, and modal mutations into one native bounds update.
    // schedule 将滚动、尺寸及模态框变化合并为一次原生边界更新。
    const schedule = (): void => {
      if (!frame) frame = window.requestAnimationFrame(update);
    };
    const resize = new ResizeObserver(schedule);
    if (placeholder.current) resize.observe(placeholder.current);
    const mutation = new MutationObserver(schedule);
    mutation.observe(document.body, {
      subtree: true, childList: true, attributes: true,
      attributeFilter: ["data-state", "aria-modal", "hidden", "style", "class"],
    });
    window.addEventListener("resize", schedule);
    window.addEventListener("scroll", schedule, true);
    schedule();
    return () => {
      resize.disconnect();
      mutation.disconnect();
      window.removeEventListener("resize", schedule);
      window.removeEventListener("scroll", schedule, true);
      if (frame) window.cancelAnimationFrame(frame);
      void bridge.viewport(null);
    };
  }, [bridge, snapshot.activeId, freeSize, deviceSize]);

  // submit opens or navigates the selected GUI-local page after address validation.
  // submit 在地址校验后打开或导航选中的 GUI 本地页面。
  // event is the address form submission; returns after the native request is queued.
  // event 是地址表单提交；在原生请求入队后返回。
  async function submit(event: React.FormEvent): Promise<void> {
    event.preventDefault();
    try {
      // The desktop host owns native path parsing; adding HTTPS here corrupts Windows drive paths.
      // 桌面宿主持有原生路径解析；在此添加 HTTPS 会破坏 Windows 盘符路径。
      const url = bridge ? address.trim() : localBrowserAddress(address);
      setError(null);
      if (bridge && !ownerReady) return;
      setAddressDraft(null);
      addressInput.current?.blur();
      if (bridge) {
        if (snapshot.activeId) await bridge.navigate(snapshot.activeId, url);
        else await bridge.open(url);
      } else {
        window.open(url, "_blank", "noopener,noreferrer");
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  // navigateHistory forwards the active page action and reports native failures in the panel.
  // navigateHistory 转发活动页面操作，并在面板中报告原生失败。
  // action is a history/loading command; returns after the bridge request completes.
  // action 是历史或加载命令；桥请求完成后返回。
  async function navigateHistory(action: "back" | "forward" | "reload" | "stop"): Promise<void> {
    if (!bridge || !activeTab || !ownerReady) return;
    setError(null);
    setAddressDraft(null);
    try { await bridge.history(activeTab.id, action); }
    catch (reason) { setError(String(reason)); }
  }

  // tabKeyDown switches tabs with arrows/Home/End and moves them with Alt+arrows.
  // tabKeyDown 用方向键、Home、End 切换标签，用 Alt 加方向键移动标签。
  // event originates from a tab button; returns nothing after handling supported keys.
  // event 来自标签按钮；处理支持的按键后无返回值。
  function tabKeyDown(event: React.KeyboardEvent<HTMLDivElement>): void {
    if (!bridge || !ownerReady || !(event.target instanceof HTMLButtonElement)
      || event.target.getAttribute("role") !== "tab") return;
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    // buttons follows the native snapshot order, excluding close and add controls.
    // buttons 遵循原生快照顺序，排除关闭与新增控件。
    const buttons = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
    // index identifies the focused tab, even when an asynchronous selection is pending.
    // index 标识聚焦的标签，即使异步选中请求仍在等待也适用。
    const index = buttons.indexOf(event.target);
    // next selects the neighboring or boundary tab in the current order.
    // next 在当前顺序中选择相邻标签或边界标签。
    const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1
      : (index + (event.key === "ArrowRight" ? 1 : -1) + buttons.length) % buttons.length;
    event.preventDefault();
    if (event.altKey) {
      void bridge.move(snapshot.tabs[index].id, snapshot.tabs[next].id).catch((reason: unknown) => setError(String(reason)));
    } else {
      buttons[next].focus();
      void bridge.select(snapshot.tabs[next].id).catch((reason: unknown) => setError(String(reason)));
    }
  }

  return <div className="flex h-full min-h-0 min-w-0 w-full flex-col gap-2 p-2">
    {/* Keep the whole strip hidden until a real destination has been opened. */}
    {/* 打开实际网址前隐藏整行标签栏。 */}
    {bridge && snapshot.tabs.some((tab) => tab.url !== "about:blank") && <div className="flex min-h-6 shrink-0 items-center gap-1 overflow-x-auto" role="tablist" onKeyDown={tabKeyDown}
      aria-label={t("browser.localBrowserTabs")}>
      {snapshot.tabs.map((tab) => <div key={tab.id} draggable={ownerReady}
        className={`context-dynamic-tab ${tab.id === snapshot.activeId ? "active" : ""} ${dropId === tab.id ? "local-browser-tab-drop" : ""}`}
        onDragStart={(event) => {
          draggedId.current = tab.id;
          event.dataTransfer.effectAllowed = "move";
          event.dataTransfer.setData("text/plain", tab.id);
        }}
        onDragOver={(event) => {
          if (!draggedId.current) return;
          event.preventDefault();
          event.dataTransfer.dropEffect = "move";
          setDropId(tab.id);
        }}
        onDrop={(event) => {
          event.preventDefault();
          if (ownerReady && draggedId.current && draggedId.current !== tab.id) {
            void bridge.move(draggedId.current, tab.id).catch((reason: unknown) => setError(String(reason)));
          }
          draggedId.current = null;
          setDropId(null);
        }}
        onDragEnd={() => { draggedId.current = null; setDropId(null); }}>
        <span className="context-dynamic-tab-icon">
          <LocalBrowserTabIcon key={tab.faviconUrl} faviconUrl={tab.faviconUrl} />
        </span>
        <button type="button"
          role="tab" aria-selected={tab.id === snapshot.activeId} tabIndex={tab.id === snapshot.activeId ? 0 : -1}
          className="context-dynamic-tab-label" title={tab.title || tab.url}
          disabled={!ownerReady}
          onClick={() => void bridge.select(tab.id).catch((reason: unknown) => setError(String(reason)))}>
          <span className="max-w-28 truncate">{tab.loading ? "… " : ""}{localBrowserTabLabel(tab.title || tab.url, t)}</span>
        </button>
        <button type="button" className="context-dynamic-tab-close"
          aria-label={t("browser.closeLocalTab")}
          disabled={!ownerReady}
          onClick={() => void bridge.close(tab.id).catch((reason: unknown) => setError(String(reason)))}><X size={12} /></button>
      </div>)}
      <button type="button" className="context-dynamic-icon-button"
        aria-label={t("browser.newLocalTab")}
        disabled={!ownerReady}
        onClick={() => void bridge.open("about:blank").catch((reason: unknown) => setError(String(reason)))}><Plus size={15} /></button>
    </div>}
    <form onSubmit={(event) => void submit(event)} className="flex shrink-0 items-center gap-1">
      {bridge && <>
        <Button type="button" variant="ghost" size="icon" className="h-7 w-7 shrink-0"
          title={t("browser.back")} aria-label={t("browser.back")}
          disabled={!ownerReady || !activeTab?.canGoBack} onClick={() => void navigateHistory("back")}><ArrowLeft className="h-4 w-4" /></Button>
        <Button type="button" variant="ghost" size="icon" className="h-7 w-7 shrink-0"
          title={t("browser.forward")} aria-label={t("browser.forward")}
          disabled={!ownerReady || !activeTab?.canGoForward} onClick={() => void navigateHistory("forward")}><ArrowRight className="h-4 w-4" /></Button>
        <Button type="button" variant="ghost" size="icon" className="h-7 w-7 shrink-0"
          title={activeTab?.loading ? t("browser.stopLoading") : t("browser.reload")}
          aria-label={activeTab?.loading ? t("browser.stopLoading") : t("browser.reload")}
          disabled={!ownerReady || !activeTab} onClick={() => void navigateHistory(activeTab?.loading ? "stop" : "reload")}>
          {activeTab?.loading ? <X className="h-4 w-4" /> : <RotateCw className="h-4 w-4" />}
        </Button>
      </>}
      <input ref={addressInput} value={address}
        onChange={(event) => setAddressDraft({ tabId: snapshot.activeId, value: event.target.value })}
        onKeyDown={(event) => { if (event.key === "Escape") { setAddressDraft(null); event.currentTarget.blur(); } }}
        aria-label={t("browser.localBrowserAddress")}
        placeholder={t("browser.enterAUrl")} className="h-8 min-w-0 flex-1 rounded border bg-background px-2 text-sm" />
      <Button type="submit" variant="outline" size="sm" className={bridge ? "sr-only" : ""} disabled={Boolean(bridge) && !ownerReady}
        aria-label={t("browser.openAddress")}>
        {bridge ? <ArrowRight className="h-4 w-4" /> : <ExternalLink className="h-4 w-4" />}
      </Button>
      {bridge && <>
        <Button type="button" variant={freeSize ? "secondary" : "ghost"} size="icon" className="h-7 w-7 shrink-0"
          title={t(freeSize ? "browser.exitResponsiveMode" : "browser.responsiveMode")}
          aria-label={t(freeSize ? "browser.exitResponsiveMode" : "browser.responsiveMode")}
          aria-pressed={freeSize} onClick={() => setFreeSize(!freeSize)}><MonitorSmartphone className="h-4 w-4" /></Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild><Button type="button" variant="ghost" size="icon" className="h-7 w-7 shrink-0"
            aria-label={t("browser.browserMenu")} disabled={!ownerReady || !activeTab}><MoreHorizontal className="h-4 w-4" /></Button></DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-56">
            <DropdownMenuItem disabled={!activeTab || !/^(?:https?|file):\/\//i.test(activeTab.url)} onSelect={() => {
              if (activeTab) void bridge.openExternal(activeTab.id).catch((reason: unknown) => setError(String(reason)));
            }}><ExternalLink className="h-4 w-4" />{t("browser.openInDefaultBrowser")}</DropdownMenuItem>
            <DropdownMenuItem onSelect={() => {
              if (activeTab) void bridge.openDeveloperTools(activeTab.id).catch((reason: unknown) => setError(String(reason)));
            }}><Bug className="h-4 w-4" />{t("browser.openDeveloperTools")}</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </>}
    </form>
    {bridge && freeSize && <div className="flex shrink-0 items-center justify-center gap-2 text-xs">
      <input type="number" min={240} max={3840} step={1} defaultValue={deviceSize.width}
        aria-label={t("browser.viewportWidth")} className="h-7 w-16 rounded border bg-background px-1 text-center"
        onBlur={(event) => {
          // width is committed only after the complete numeric edit is valid.
          // width 仅在完整的数字编辑有效后提交。
          const width = event.currentTarget.valueAsNumber;
          if (Number.isInteger(width) && width >= 240 && width <= 3840) setDeviceSize((size) => ({ ...size, width }));
          else event.currentTarget.value = String(deviceSize.width);
        }} onKeyDown={(event) => { if (event.key === "Enter") event.currentTarget.blur(); }} />
      <span>×</span>
      <input type="number" min={240} max={3840} step={1} defaultValue={deviceSize.height}
        aria-label={t("browser.viewportHeight")} className="h-7 w-16 rounded border bg-background px-1 text-center"
        onBlur={(event) => {
          // height follows the same explicit bounds as the native viewport contract.
          // height 遵循与原生视口契约相同的明确范围。
          const height = event.currentTarget.valueAsNumber;
          if (Number.isInteger(height) && height >= 240 && height <= 3840) setDeviceSize((size) => ({ ...size, height }));
          else event.currentTarget.value = String(deviceSize.height);
        }} onKeyDown={(event) => { if (event.key === "Enter") event.currentTarget.blur(); }} />
      <span className="text-muted-foreground">{t("browser.fitToWindow")}</span>
    </div>}
    {error && <p role="alert" className="text-xs text-destructive">{error}</p>}
    {bridge ? <div ref={placeholder} className="min-h-0 w-full flex-1 overflow-hidden rounded bg-muted/20">
      {!activeTab && <div className="flex h-full flex-col items-center justify-center gap-3 p-3 text-center text-sm text-muted-foreground">
        <Globe className="h-12 w-12 opacity-50" />
        <p>{t("browser.pasteOrEnterA")}</p>
      </div>}
    </div> : <div className="flex min-h-0 flex-1 flex-col gap-2 rounded border p-3 text-sm text-muted-foreground">
      <p>{t("browser.thisClientOpensTemporary")}</p>
      {(() => {
        try {
          const url = localBrowserAddress(address);
          return <a href={url} target="_blank" rel="noopener noreferrer" className="break-all text-primary underline">
            {t("browser.openLinkDirectly")}: {url}
          </a>;
        } catch {
          return null;
        }
      })()}
    </div>}
  </div>;
}
