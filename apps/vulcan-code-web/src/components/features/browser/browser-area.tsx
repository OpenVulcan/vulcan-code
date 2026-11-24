import * as React from "react";
import { ArrowRight, Download, Globe, Keyboard, MoreHorizontal, RefreshCw, Settings2, X } from "lucide-react";
import { PageEmptyState, PageNotice } from "@/components/page";
import { BROWSER_HOST_MAX_VIEWPORT_DIMENSION } from "@/generated/protocol-limits";
import { errorMessage } from "@/lib/display-format";

import { Button } from "@/ui-kit/primitives/button";
import { CoreBrowserRuntimeControls } from "./core-browser-runtime-controls";
import { BrowserDomAssets, type BrowserDomReply, type BrowserDomRevision, type BrowserDomUpdate, type BrowserDomViewport } from "./browser-dom-assets";
import { BrowserDomSurface } from "./browser-dom-surface";
import { BrowserViewportMenu } from "./browser-viewport-menu";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/ui-kit/primitives/dropdown-menu";
import { formatMessage, useI18n } from "@/i18n";
import {
  requestCoreBrowser,
  fetchCoreBrowserDownload,
  type CoreBrowserDownload,
  type CoreBrowserDialog,
  type CoreBrowserFileChooser,
  type CoreBrowserPage,
  type CoreBrowserSession,
} from "@/service/core-browser-service";
import {
  BrowserCloseConfirmDialog,
  BrowserInstanceTabs,
  BrowserPagePrompts,
  BrowserPageTabs,
  browserTabTitle,
  type BrowserCloseRequest,
} from "./browser-area-sections";
import { useBrowserInputQueue, type HeldBrowserPointer, type QueuedBrowserInput } from "./use-browser-input-queue";

import {
  observeCoreBrowserSessions, observeCoreBrowserPages, observeCoreBrowserDownloads,
  observeCoreBrowserDialogs, observeCoreBrowserFileChoosers,
  refreshCoreBrowserSessions, refreshCoreBrowserPages,
} from "@/service/core-browser-metadata";
import "@/ui-kit/styles/features/browser.css"

// BrowserPollSource identifies independently refreshed Core browser metadata.
// BrowserPollSource 标识独立刷新的 Core 浏览器元数据来源。
type BrowserPollSource = "sessions" | "pages" | "downloads" | "fileChoosers" | "dialogs";

// coreBrowserAddress turns a human-entered domain into the complete URL required by Core.
// coreBrowserAddress 将用户输入的域名转为 Core 契约要求的完整地址。
// input is the address bar text; returns an HTTP(S) URL or about:blank, or throws a visible error.
// input 是地址栏文本；返回 HTTP(S) 地址或 about:blank，非法输入则抛出可见错误。
export function coreBrowserAddress(input: string): string {
  const value = input.trim();
  if (value === "about:blank") return value;
  if (!value) throw new Error("Enter a website address.");
  const loopback = /^(?:localhost|127(?:\.\d{1,3}){3}|\[::1\])(?::|\/|$)/i.test(value);
  const complete = /^https?:\/\//i.test(value) ? value : /^[a-z][a-z\d+.-]*:\/\//i.test(value)
    ? value : `${loopback ? "http" : "https"}://${value}`;
  const url = new URL(complete);
  if ((url.protocol !== "http:" && url.protocol !== "https:") || !url.hostname) {
    throw new Error("Only HTTP and HTTPS website addresses can be opened.");
  }
  return url.href;
}

// Keep the synchronous input gate and its rendered status in step while Core confirms the projection.
// 等待 Core 确认投影时，同步更新输入门控与界面等待状态。
function useProjectionSuspension() {
  // Input callbacks must see the gate immediately, before React renders the pending status.
  // 输入回调必须在 React 渲染等待状态之前立即看到门控。
  const projectionSuspended = React.useRef(false);
  // The menu renders whether a matching projection is still pending.
  // 菜单显示是否仍在等待匹配的投影。
  const [awaitingProjection, setAwaitingProjection] = React.useState(false);
  // Set both consumers to the same pending state.
  // 将两个消费者设为相同的等待状态。
  const suspendProjection = React.useCallback((pending: boolean) => {
    projectionSuspended.current = pending;
    setAwaitingProjection(pending);
  }, []);
  return { projectionSuspended, awaitingProjection, suspendProjection };
}

// BrowserArea lets any WebUI client observe and control a Core-owned browser page.
// BrowserArea 让任意 WebUI 客户端观察和控制 Core 拥有的浏览器页面。
export function BrowserArea(props: { ownerSessionId: string }) {
  // t selects labels without changing the Core wire protocol.
  // t 选择文案而不改变 Core 线协议。
  const { t } = useI18n();
  // sessions is the latest Core-owned session list.
  // sessions 是最新的 Core 自有会话列表。
  const [sessions, setSessions] = React.useState<CoreBrowserSession[]>([]);
  // selectedSessionId is this client's UI selection, independent of AI ownership.
  // selectedSessionId 是本客户端的界面选择，与 AI 归属无关。
  const [selectedSessionId, setSelectedSessionId] = React.useState<string | null>(null);
  // pages is the selected session's current page registry.
  // pages 是选定会话当前的页面注册表。
  const [pages, setPages] = React.useState<CoreBrowserPage[]>([]);
  // viewedPages remembers each browser's last displayed page, independently of AI ownership.
  // viewedPages 记录每个浏览器最后显示的页面，与 AI 控制权无关。
  const viewedPages = React.useRef(new Map<string, CoreBrowserPage>());
  // downloads are path-free records from the selected Core session.
  // downloads 是选定 Core 会话提供的不含路径的记录。
  const [downloads, setDownloads] = React.useState<CoreBrowserDownload[]>([]);
  // dialogs are pending page questions visible to observers but answered only by the lease holder.
  // dialogs 是观察者可见、但仅由租约持有者答复的待处理页面问题。
  const [dialogs, setDialogs] = React.useState<CoreBrowserDialog[]>([]);
  // fileChoosers identify clicked page inputs waiting for this controller's local file.
  // fileChoosers 标识等待当前控制者选择本地文件的已点击页面输入。
  const [fileChoosers, setFileChoosers] = React.useState<CoreBrowserFileChooser[]>([]);
  // dialogText is the controller's editable answer to a prompt dialog.
  // dialogText 是控制者对提示框的可编辑答复。
  const [dialogText, setDialogText] = React.useState("");
  // selectedPageId is this observer's page choice, not the AI's automation target.
  // selectedPageId 是本观察端的页面选择，而非 AI 的自动化目标。
  const [selectedPageId, setSelectedPageId] = React.useState<string | null>(null);
  // Auxiliary controls open explicitly so the page keeps the available panel area.
  // 辅助控件按需展开，让页面占据可用面板区域。
  // A pending resize is scoped to its initiating browser, even when selection changes.
  // 在途调整限定在发起操作的浏览器，切换选择也不会改变其归属。
  const [resizing, setResizing] = React.useState<ReadonlySet<string>>(new Set());
  const resizePending = React.useRef(new Set<string>());
  // Read only the drawable panel area when the fit action is explicitly invoked.
  // 只在用户明确触发适配操作时读取面板可绘制区域。
  const viewportPanel = React.useRef<HTMLDivElement>(null);
  const [showTextInput, setShowTextInput] = React.useState(false);
  const [showDownloads, setShowDownloads] = React.useState(false);
  const [showRuntime, setShowRuntime] = React.useState(false);
  // closeRequest freezes the confirmed browser/page identity even when polls update the selection.
  // closeRequest 冻结待确认的浏览器、页面身份，避免轮询变更选择后关错目标。
  const [closeRequest, setCloseRequest] = React.useState<BrowserCloseRequest | null>(null);
  // closing and closePending prevent duplicate destructive requests while keeping errors in the dialog.
  // closing 和 closePending 阻止重复关闭请求，并让错误保留在确认框内。
  const [closing, setClosing] = React.useState(false);
  const closePending = React.useRef(false);
  const [closeError, setCloseError] = React.useState<string | null>(null);
  // address is the editable navigation destination.
  // address 是可编辑的导航目标地址。
  const [address, setAddress] = React.useState("about:blank");
  // textInput is committed user text sent to the browser page, including IME text.
  // textInput 是发送到浏览器页面的用户已提交文本，包括输入法文本。
  const [textInput, setTextInput] = React.useState("");
  // The rendered revision and its geometry jointly identify the visible page.
  // 已渲染修订及其视口尺寸共同标识当前可见页面。
  const [dom, setDom] = React.useState<(BrowserDomRevision & BrowserDomViewport) | null>(null);
  const hasFrame = dom !== null && dom.pageId === selectedPageId;
  // streamStatus names the current observation connection state.
  // streamStatus 命名当前观察连接状态。
  const [streamStatus, setStreamStatus] = React.useState("disconnected");
  // Projection errors are cleared by the next successful read independently of input errors.
  // 投影错误由下一次成功读取独立清除，不影响输入错误。
  const [domError, setDomError] = React.useState<string | null>(null);
  // error keeps a refused or uncertain operation visible to the user.
  // error 让被拒绝或结果未知的操作保持对用户可见。
  const [error, setError] = React.useState<string | null>(null);
  // pollErrors keep recoverable list failures separate from explicit operation results.
  // pollErrors 将可恢复的列表失败与显式操作结果分开保存。
  const [pollErrors, setPollErrors] = React.useState<Record<BrowserPollSource, string | null>>({
    sessions: null, pages: null, downloads: null, fileChoosers: null, dialogs: null,
  });
  // frameRef gives the input queue the exact generations of the visible DOM viewport.
  // frameRef 为输入队列提供可见 DOM 视口的确切代次。
  const frameRef = React.useRef<(BrowserDomViewport & Pick<BrowserDomRevision, "documentId">) | null>(null);
  // requiredFrame blocks input after a page or viewport mutation until a frame from its new generation arrives.
  // requiredFrame 在页面或视口变化后阻止输入，直至新代次画面到达。
  const requiredFrame = React.useRef<{ pageGeneration: string; viewportGeneration: string } | null>(null);
  // The input gate also covers the interval before Core returns the new generation.
  // 输入门控也覆盖 Core 返回新代次之前的时段。
  const { projectionSuspended, awaitingProjection, suspendProjection } = useProjectionSuspension();
  // pendingPointerMove retains only the latest not-yet-dispatched drag position.
  // pendingPointerMove 仅保留尚未派发的最新拖拽位置。
  const pendingPointerMove = React.useRef<QueuedBrowserInput | null>(null);
  // heldKeys lets focus loss release only keys this view actually pressed.
  // heldKeys 使失焦时只释放此视图确实按下过的按键。
  const heldKeys = React.useRef(new Set<string>());
  // heldPointer lets a drag finish after the pointer leaves the projected image.
  // heldPointer 允许指针离开投影画面后仍完成拖拽释放。
  const heldPointer = React.useRef<HeldBrowserPointer | null>(null);
  // addressRef prevents background page refresh from replacing text during editing.
  // addressRef 防止后台页面刷新覆盖正在编辑的地址。
  const addressRef = React.useRef<HTMLInputElement | null>(null);
  // Metadata observers use the latest page switch so held input is released before following.
  // 元数据观察器使用最新的页面切换回调，保证跟随前释放已按下的输入。
  const selectPageRef = React.useRef<(page: CoreBrowserPage) => void>(null);
  // selectedSessionRef rejects responses belonging to a browser that is no longer selected.
  // selectedSessionRef 拒绝已不再选中的浏览器所产生的迟到响应。
  const selectedSessionRef = React.useRef(selectedSessionId);
  // selectedPageRef rejects frames from a page that is no longer selected.
  // selectedPageRef 拒绝已不再选中的页面所产生的迟到画面。
  const selectedPageRef = React.useRef(selectedPageId);
  // ownerSessionRef rejects responses after the panel changes conversation ownership.
  // ownerSessionRef 在面板切换会话归属后拒绝旧会话响应。
  const ownerSessionRef = React.useRef(props.ownerSessionId);
  // componentMounted rejects late browser operations after this panel is removed.
  // componentMounted 在面板卸载后拒绝迟到的浏览器操作。
  const componentMounted = React.useRef(true);

  // selectedSession is the latest Core projection for this UI choice.
  // selectedSession 是本界面所选对象的最新 Core 投影。
  const selectedSession = sessions.find((session) => session.sessionId === selectedSessionId) ?? null;
  // selectedPage is the exact Core page currently observed here.
  // selectedPage 是此处观察的精确 Core 页面。
  const selectedPage = pages.find((page) => page.pageId === selectedPageId) ?? null;
  // visibleSessions includes failed browsers so their diagnostics remain reachable.
  // visibleSessions 保留故障浏览器，让诊断信息仍可访问。
  const visibleSessions = sessions.filter((session) => session.status === "running" || session.status === "crashed");
  // A live, unblocked projection can accept human actions directly.
  // 实时且未阻塞的投影可直接接收人工操作。
  const interactive = selectedSession?.status === "running" && selectedSession.leaseState !== "blocked";
  // Only the browser with the pending write pauses its local interaction controls.
  // 仅暂停正在调整尺寸的浏览器本地交互控件。
  const resizingSelection = selectedSessionId !== null && resizing.has(selectedSessionId);

  // Page input and toolbar actions share one short-lived control queue.
  // 页面输入与工具栏操作共用一条临时控制队列。
  const { queueInput, queuePointerMove, runWithControl, resetControl } = useBrowserInputQueue({
    selectedSession, frameRef, projectionSuspended, ownerSessionRef, selectedSessionRef, selectedPageRef,
    componentMounted, pendingPointerMove, heldKeys, heldPointer, setError,
  });

  // isCurrentOwner checks whether an asynchronous operation still belongs to this conversation.
  // isCurrentOwner 检查异步操作是否仍属于当前会话；返回当前归属是否一致。
  function isCurrentOwner(): boolean {
    return componentMounted.current && ownerSessionRef.current === props.ownerSessionId;
  }

  // isCurrentSession checks the trusted owner and one selected Core browser identity.
  // isCurrentSession 检查可信归属及一个选定的 Core 浏览器身份；返回选择是否仍有效。
  function isCurrentSession(sessionId: string): boolean {
    return isCurrentOwner() && selectedSessionRef.current === sessionId;
  }

  // isCurrentPage checks the trusted owner, Core browser, and page identities together.
  // isCurrentPage 同时检查可信归属、Core 浏览器和页面身份；返回选择是否仍有效。
  function isCurrentPage(sessionId: string, pageId: string): boolean {
    return isCurrentSession(sessionId) && selectedPageRef.current === pageId;
  }

  React.useLayoutEffect(() => {
    // An owner change must revoke the old visible projection and release its control grant before paint.
    // 归属切换必须在绘制前撤销旧投影，并交还旧归属的控制授权。
    if (ownerSessionRef.current !== props.ownerSessionId) {
      setSessions([]);
      setCloseRequest(null);
      setCloseError(null);
      setClosing(false);
      viewedPages.current.clear();
      setSelectedSessionId(null);
      setAddress("about:blank");
      setTextInput("");
      setDialogText("");
      setError(null);
      setStreamStatus("disconnected");
    }
    // Commit the exact selection before any pending response can publish into the new view.
    // 在迟到响应写入新视图之前提交精确的归属与选择。
    ownerSessionRef.current = props.ownerSessionId;
    selectedSessionRef.current = selectedSessionId;
    selectedPageRef.current = selectedPageId;
  }, [props.ownerSessionId, selectedSessionId, selectedPageId]);

  React.useLayoutEffect(() => {
    if (selectedSessionId && selectedPage) viewedPages.current.set(selectedSessionId, selectedPage);
  }, [selectedSessionId, selectedPage]);

  React.useLayoutEffect(() => {
    // Clear the old browser before paint so a changed selection never displays its metadata.
    // 在绘制前清空旧浏览器，避免切换选择后显示其元数据。
    resetControl();
    setPages([]);
    setDownloads([]);
    setDialogs([]);
    setFileChoosers([]);
    setAddress("about:blank");
    setPollErrors({ sessions: null, pages: null, downloads: null, fileChoosers: null, dialogs: null });
    setSelectedPageId(null);
  }, [props.ownerSessionId, selectedSessionId]);

  React.useLayoutEffect(() => {
    // A page switch must hide the previous frame before its old stream cleanup runs.
    // 页面切换时须在旧流清理运行前隐藏上一页画面。
    frameRef.current = null;
    setDom(null);
  }, [props.ownerSessionId, selectedSessionId, selectedPageId]);

  // Explicit refreshes share the same ordered read stream as every mounted observer.
  // 显式刷新与所有已挂载观察者共用相同的有序读取流。
  const refreshSessions = React.useCallback(async () => {
    await refreshCoreBrowserSessions(props.ownerSessionId);
  }, [props.ownerSessionId]);
  const refreshPages = React.useCallback(async (sessionId: string) => {
    await refreshCoreBrowserPages(props.ownerSessionId, sessionId);
  }, [props.ownerSessionId]);

  React.useEffect(() => {
    let cancelled = false;
    const stop = observeCoreBrowserSessions(props.ownerSessionId, {
      next(next) {
        if (cancelled || ownerSessionRef.current !== props.ownerSessionId) return;
        setSessions(next);
        setPollErrors(current => current.sessions === null ? current : { ...current, sessions: null });
        const available = next.filter(session => session.status === "running" || session.status === "crashed");
        setSelectedSessionId(current => current && available.some(session => session.sessionId === current)
          ? current : available.length === 1 ? available[0].sessionId : null);
      },
      error(failure) {
        if (!cancelled && ownerSessionRef.current === props.ownerSessionId) {
          setPollErrors(current => ({ ...current, sessions: String(failure) }));
        }
      },
    });
    return () => { cancelled = true; stop(); };
  }, [props.ownerSessionId]);

  React.useEffect(() => {
    if (!selectedSessionId) return;
    let cancelled = false;
    // Compare consecutive registries so initial hydration and manual selection do not steal focus.
    // 比较相邻的注册表，避免初次加载和手动选择被自动跟随打断。
    let knownPages: Set<string> | null = null;
    const live = () => !cancelled && ownerSessionRef.current === props.ownerSessionId
      && selectedSessionRef.current === selectedSessionId;
    const observer = <T,>(source: BrowserPollSource, apply: (value: T[]) => void) => ({
      next(value: T[]) {
        if (!live()) return;
        apply(value);
        setPollErrors(current => current[source] === null ? current : { ...current, [source]: null });
      },
      error(failure: unknown) {
        if (live()) setPollErrors(current => ({ ...current, [source]: String(failure) }));
      },
    });
    const stops = [
      observeCoreBrowserPages(props.ownerSessionId, selectedSessionId, observer("pages", (next: CoreBrowserPage[]) => {
        setPages(next);
        // Core assigns monotonically increasing page-N IDs even after a host restart.
        // Core 分配单调递增的 page-N 标识，即使宿主重启也是如此。
        const added = knownPages ? next.filter(page => !knownPages?.has(page.pageId)) : [];
        knownPages = new Set(next.map(page => page.pageId));
        const newest = added.reduce<CoreBrowserPage | null>((latest, page) =>
          !latest || Number(page.pageId.slice(5)) > Number(latest.pageId.slice(5)) ? page : latest, null);
        if (newest && selectPageRef.current) {
          selectPageRef.current(newest);
          return;
        }
        setSelectedPageId(current => {
          const remembered = viewedPages.current.get(selectedSessionId);
          if (current && next.some(page => page.pageId === current)) return current;
          if (remembered && next.some(page => page.pageId === remembered.pageId)) return remembered.pageId;
          return next[0]?.pageId ?? null;
        });
      })),
      observeCoreBrowserDownloads(props.ownerSessionId, selectedSessionId, observer("downloads", setDownloads)),
      observeCoreBrowserFileChoosers(props.ownerSessionId, selectedSessionId, observer("fileChoosers", setFileChoosers)),
      observeCoreBrowserDialogs(props.ownerSessionId, selectedSessionId, observer("dialogs", setDialogs)),
    ];
    return () => { cancelled = true; stops.forEach(stop => stop()); };
  }, [props.ownerSessionId, selectedSessionId]);

  React.useEffect(() => {
    setDialogText(dialogs.find((dialog) => dialog.pageId === selectedPageId)?.defaultPrompt ?? "");
  }, [dialogs.map((dialog) => dialog.dialogId).join("|"), selectedPageId]);

  React.useLayoutEffect(() => {
    if (selectedPage && document.activeElement !== addressRef.current) {
      setAddress(selectedPage.url || "about:blank");
    }
  }, [selectedPage]);

  React.useEffect(() => {
    if (!selectedSessionId || !selectedPageId) {
      return;
    }
    suspendProjection(resizePending.current.has(selectedSessionId));
    requiredFrame.current = null;
    const sessionId = selectedSessionId;
    const pageId = selectedPageId;
    const owner = props.ownerSessionId;
    const assets = new BrowserDomAssets();
    let timer: ReturnType<typeof setTimeout>;
    let active = true;
    let revision: string | null = null;
    let markup = "";
    let delay = 120;
    const live = () => active && ownerSessionRef.current === owner
      && selectedSessionRef.current === sessionId && selectedPageRef.current === pageId;
    setStreamStatus("connecting");
    setDomError(null);

    // Keep one read in flight and use a conditional revision for quiet pages.
    // 保持至多一项在途读取，并对静止页面使用条件修订号。
    async function refreshDom(): Promise<void> {
      try {
        const reply = await requestCoreBrowser<BrowserDomReply<BrowserDomUpdate>>(owner,
          "browser.dom.snapshot", { sessionId, pageId, revision: frameRef.current && !projectionSuspended.current ? revision : null });
        if (!live() || reply.pageId !== pageId || !reply.result) return;
        const required = requiredFrame.current;
        const suspended = projectionSuspended.current;
        if (suspended && (!required
          || BigInt(reply.pageGeneration) < BigInt(required.pageGeneration)
          || BigInt(reply.viewportGeneration) < BigInt(required.viewportGeneration))) return;
        if ("documentId" in reply.result) {
          const update = reply.result;
          const next = "html" in update ? update : {
            ...update, html: markup.slice(0, update.patch.start) + update.patch.text + markup.slice(update.patch.end),
          };
          const html = await assets.render(owner, sessionId, pageId, next);
          if (!live() || requiredFrame.current !== required || projectionSuspended.current !== suspended) return;
          // Navigation replaces the frame and may remove its listeners before a physical release.
          // 导航会替换框架，可能在物理输入释放前移除监听器。
          if (frameRef.current && frameRef.current.documentId !== next.documentId
            && (heldPointer.current || heldKeys.current.size > 0)) resetControl();
          const viewport = { pageId, documentId: next.documentId, pageGeneration: reply.pageGeneration,
            viewportGeneration: reply.viewportGeneration, width: next.width, height: next.height };
          frameRef.current = viewport;
          revision = next.revision;
          markup = next.html;
          suspendProjection(false);
          requiredFrame.current = null;
          setDom({ ...next, ...viewport, html });
        } else if (frameRef.current && !projectionSuspended.current) {
          frameRef.current = { ...frameRef.current, pageGeneration: reply.pageGeneration, viewportGeneration: reply.viewportGeneration };
        }
        delay = 120;
        setStreamStatus("connected");
        setDomError(null);
      } catch (failure) {
        if (live()) {
          setStreamStatus("connecting");
          delay = Math.min(delay * 2, 1500);
          if (delay >= 1500) setDomError(String(failure));
        }
      } finally {
        if (live()) timer = setTimeout(() => void refreshDom(), delay);
      }
    }
    void refreshDom();
    return () => {
      active = false;
      clearTimeout(timer);
      assets.close();
      frameRef.current = null;
      setDom(null);
    };
  }, [props.ownerSessionId, selectedPageId, selectedSessionId]);

  React.useEffect(() => {
    componentMounted.current = true;
    return () => {
      // Drop local input and return the exact grant when this view is removed.
      // 卸载视图时丢弃本地输入并交还精确授权。
      componentMounted.current = false;
      resetControl();
      selectedSessionRef.current = null;
      selectedPageRef.current = null;
    };
  }, []);

  // Switch the observation immediately. The action queue returns the previous lease in order.
  // 立即切换观察对象，由操作队列按顺序交还上一个租约。
  async function selectSession(sessionId: string): Promise<void> {
    if (selectedSessionRef.current === sessionId) return;
    selectedSessionRef.current = sessionId;
    setSelectedSessionId(sessionId);
    setSelectedPageId(null);
    setError(null);
  }

  // selectPage finishes held key and pointer input on the previous page before observing another page tab.
  // selectPage 在观察另一页面页签前，先在上一页面完成已按下的按键与指针输入。
  function selectPage(page: CoreBrowserPage): void {
    if (page.pageId === selectedPageRef.current) return;
    for (const key of heldKeys.current) queueInput("key", { type: "up", key });
    heldKeys.current.clear();
    const held = heldPointer.current;
    if (held) queueInput("pointer", { type: "up", button: held.button, x: held.x, y: held.y });
    heldPointer.current = null;
    selectedPageRef.current = page.pageId;
    setSelectedPageId(page.pageId);
  }
  selectPageRef.current = selectPage;

  // navigate changes only the selected Core page after a real host confirmation.
  // navigate 仅在真实宿主确认后改变选定的 Core 页面。
  async function navigate(event: React.FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    if (!interactive || !selectedSession || !selectedSessionId || !selectedPageId || resizePending.current.has(selectedSessionId)) {
      return;
    }
    // destination must be validated before hiding the current frame; a bare domain gets HTTPS.
    // destination 必须在隐藏当前画面前完成校验；无协议域名自动补全 HTTPS。
    let destination: string;
    try {
      destination = coreBrowserAddress(address);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
      return;
    }
    setAddress(destination);
    try {
      const updated = await runWithControl(selectedSession, () => isCurrentPage(selectedSessionId, selectedPageId), lease =>
        requestCoreBrowser<CoreBrowserSession>(props.ownerSessionId, "browser.session.navigate", {
          sessionId: selectedSessionId, pageId: selectedPageId, leaseSecret: lease.leaseSecret, url: destination,
        }));
      if (!updated || !isCurrentPage(selectedSessionId, selectedPageId)) return;
      // Keep the current projection until Core confirms the navigation and supplies a new generation.
      // 保留当前投影，直到 Core 确认导航并提供新代次。
      if (heldPointer.current || heldKeys.current.size > 0) resetControl();
      suspendProjection(true);
      requiredFrame.current = { pageGeneration: updated.pageGeneration, viewportGeneration: updated.viewportGeneration };
      frameRef.current = null;
      setDom(null);
      setSessions((current) => current.map((session) => session.sessionId === updated.sessionId ? updated : session));
      await refreshPages(selectedSessionId);
      if (!isCurrentPage(selectedSessionId, selectedPageId)) return;
      await refreshSessions();
      if (isCurrentPage(selectedSessionId, selectedPageId)) setError(null);
    } catch (failure) {
      if (isCurrentPage(selectedSessionId, selectedPageId)) {
        setError(errorMessage(failure));
      }
    }
  }

  // saveDownload transfers one Core-authenticated file into the user's local downloads.
  // saveDownload 将一个经 Core 鉴权的文件保存到用户本地下载位置。
  async function saveDownload(download: CoreBrowserDownload): Promise<void> {
    try {
      const blob = await fetchCoreBrowserDownload(props.ownerSessionId, download.downloadId);
      if (!isCurrentSession(download.sessionId)) return;
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = download.fileName;
      anchor.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 60_000);
      setError(null);
    } catch (failure) {
      if (isCurrentSession(download.sessionId)) setError(String(failure));
    }
  }

  // answerDialog submits the current controller's exact dialog decision to Core.
  // answerDialog 将当前控制者对精确对话框的决定提交给 Core。
  async function answerDialog(dialog: CoreBrowserDialog, accept: boolean): Promise<void> {
    if (!interactive || selectedSession?.sessionId !== dialog.sessionId) return;
    try {
      const answered = await runWithControl(selectedSession, () => isCurrentPage(dialog.sessionId, dialog.pageId), async lease => {
        await requestCoreBrowser(props.ownerSessionId, "browser.dialog.respond", {
          sessionId: dialog.sessionId, pageId: dialog.pageId, dialogId: dialog.dialogId,
          leaseSecret: lease.leaseSecret, accept, promptText: dialog.dialogType === "prompt" && accept ? dialogText : null,
        });
        return true;
      });
      if (!answered || !isCurrentPage(dialog.sessionId, dialog.pageId)) return;
      setDialogs((current) => current.filter((item) => item.dialogId !== dialog.dialogId));
      setError(null);
    } catch (failure) {
      if (isCurrentPage(dialog.sessionId, dialog.pageId)) setError(errorMessage(failure));
    }
  }

  // submitFileChooser stages one local file at Core and binds it to the exact clicked input.
  // submitFileChooser 在 Core 暂存一个本地文件，并绑定到精确的已点击输入。
  async function submitFileChooser(chooser: CoreBrowserFileChooser, file: File | undefined): Promise<void> {
    if (!file || !interactive || selectedSession?.sessionId !== chooser.sessionId) return;
    if (file.size > 8 * 1024 * 1024) {
      setError(t("browser.oneFileCannotExceed"));
      return;
    }
    try {
      const dataBase64 = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onerror = () => reject(reader.error);
        reader.onload = () => {
          if (typeof reader.result !== "string") {
            reject(new Error("File read returned no data URL."));
            return;
          }
          resolve(reader.result.slice(reader.result.indexOf(",") + 1));
        };
        reader.readAsDataURL(file);
      });
      const stillChoosing = () => isCurrentPage(chooser.sessionId, chooser.pageId);
      const attached = await runWithControl(selectedSession, stillChoosing, async lease => {
        const staged = await requestCoreBrowser<{ uploadId: string }>(props.ownerSessionId, "browser.upload.stage", {
          sessionId: chooser.sessionId, leaseSecret: lease.leaseSecret, name: file.name, dataBase64,
        });
        if (!stillChoosing()) return false;
        await requestCoreBrowser(props.ownerSessionId, "browser.upload.attach", {
          sessionId: chooser.sessionId, pageId: chooser.pageId, chooserId: chooser.chooserId,
          leaseSecret: lease.leaseSecret, uploadId: staged.uploadId,
        });
        return true;
      });
      if (!attached || !stillChoosing()) return;
      setFileChoosers((current) => current.filter((item) => item.chooserId !== chooser.chooserId));
      setError(null);
    } catch (failure) {
      if (isCurrentPage(chooser.sessionId, chooser.pageId)) setError(errorMessage(failure));
    }
  }

  // Resize the selected Core browser once and resume input with a confirmed viewport generation.
  // 对选定 Core 浏览器调整一次尺寸，并在取得确认的视口代次后恢复输入。
  async function resizeViewport({ width, height }: NonNullable<CoreBrowserSession["viewportSize"]>): Promise<void> {
    const sessionId = selectedSessionId;
    if (!interactive || !selectedSession || !sessionId || resizePending.current.has(sessionId)
      || !isCurrentSession(sessionId) || projectionSuspended.current) return;
    resizePending.current.add(sessionId);
    setResizing(new Set(resizePending.current));
    // Keep the previous DOM mounted until the matching snapshot arrives.
    // 在匹配快照到达前保留现有 DOM。
    for (const key of heldKeys.current) void queueInput("key", { type: "up", key });
    heldKeys.current.clear();
    const held = heldPointer.current;
    if (held) void queueInput("pointer", { type: "up", button: held.button, x: held.x, y: held.y });
    heldPointer.current = null;
    suspendProjection(true);
    requiredFrame.current = null;
    try {
      const updated = await runWithControl(selectedSession, () => isCurrentSession(sessionId), lease =>
        requestCoreBrowser<CoreBrowserSession>(props.ownerSessionId, "browser.viewport.set", {
          sessionId, leaseSecret: lease.leaseSecret, width, height,
        }));
      if (!isCurrentSession(sessionId)) return;
      if (!updated) {
        suspendProjection(false);
        return;
      }
      requiredFrame.current = { pageGeneration: updated.pageGeneration, viewportGeneration: updated.viewportGeneration };
      setSessions((current) => current.map((session) => session.sessionId === updated.sessionId ? updated : session));
      setError(null);
      void refreshSessions().catch(() => {});
    } catch (failure) {
      if (isCurrentSession(sessionId)) {
        suspendProjection(false);
        setError(errorMessage(failure));
      }
    } finally {
      resizePending.current.delete(sessionId);
      if (componentMounted.current) setResizing(new Set(resizePending.current));
    }
  }

  // Read the panel content dimensions in the accepted CSS-pixel range, or null when hidden.
  // 读取有效 CSS 像素范围内的面板内容尺寸，隐藏时返回空值。
  function readPanelSize(): CoreBrowserSession["viewportSize"] {
    const panel = viewportPanel.current;
    if (!panel || panel.clientWidth <= 0 || panel.clientHeight <= 0) return null;
    return {
      width: Math.min(panel.clientWidth, BROWSER_HOST_MAX_VIEWPORT_DIMENSION),
      height: Math.min(panel.clientHeight, BROWSER_HOST_MAX_VIEWPORT_DIMENSION),
    };
  }

  // closeSession closes the exact confirmed page or browser; no arguments, resolves after Core confirms.
  // closeSession 关闭精确确认的页面或浏览器；无参数，Core 确认后完成。
  async function closeSession(): Promise<void> {
    if (!closeRequest || closePending.current || closeRequest.session.ownerSessionId !== props.ownerSessionId) return;
    const { session, page } = closeRequest;
    const sessionId = session.sessionId;
    closePending.current = true;
    setClosing(true);
    setCloseError(null);
    try {
      // Closing an unselected tab still targets the exact page frozen by the confirmation.
      // 关闭未选中标签时，仍以确认框冻结的精确页面为目标。
      const closed = await runWithControl(session, isCurrentOwner, async lease => {
        await requestCoreBrowser(props.ownerSessionId, page ? "browser.page.close" : "browser.session.close", {
          sessionId,
          leaseSecret: lease.leaseSecret,
          ...(page ? { pageId: page.pageId } : {}),
        });
        return true;
      }, page === null);
      if (!closed || !isCurrentOwner()) return;
      if (!page) {
        viewedPages.current.delete(sessionId);
        if (isCurrentSession(sessionId)) {
          setSelectedSessionId(null);
          setSelectedPageId(null);
        }
      } else if (isCurrentSession(sessionId)) {
        await refreshPages(sessionId);
      }
      await refreshSessions();
      if (isCurrentOwner()) { setError(null); setCloseRequest(null); }
    } catch (failure) {
      if (isCurrentOwner()) setCloseError(errorMessage(failure));
    } finally {
      closePending.current = false;
      if (isCurrentOwner()) setClosing(false);
    }
  }

  return (
    <div className="vk-browser" aria-label={t("browser.aiBrowser")}>
      <CoreBrowserRuntimeControls compact={!showRuntime} />
      <BrowserInstanceTabs
        visibleSessions={visibleSessions}
        selectedSessionId={selectedSessionId}
        selectedPage={selectedPage}
        viewedPages={viewedPages.current}
        selectSession={selectSession}
        setCloseError={setCloseError}
        setCloseRequest={setCloseRequest}
      />
      {selectedSession ? (
        <>
          {selectedSession.status === "crashed" ? <PageNotice tone="error">
            {t("browser.thePlaywrightBrowserHost")}
          </PageNotice> : null}
          {selectedSession.blockedReason ? <p className="vk-field-error">{selectedSession.blockedReason}</p> : null}
          <BrowserPageTabs
            pages={pages}
            selectedPageId={selectedPageId}
            selectedSession={selectedSession}
            selectPage={selectPage}
            setCloseError={setCloseError}
            setCloseRequest={setCloseRequest}
          />
        </>
      ) : null}
      <div className="vk-browser-bar">
        {selectedSession ? <form onSubmit={(event) => void navigate(event)}>
            <input ref={addressRef} className="vk-browser-address" value={address}
              onChange={(event) => setAddress(event.target.value)} aria-label={t("browser.pageUrl")} />
            <Button type="submit" size="icon-sm" variant="ghost" title={t("browser.go")} aria-label={t("browser.go")} disabled={!interactive || !selectedPage || resizingSelection}><ArrowRight size={14} /></Button>
          </form> : <span className="vk-browser-fill" />}
        {selectedSession ? <BrowserViewportMenu key={`${props.ownerSessionId}:${selectedSession.sessionId}`}
          size={dom && dom.pageId === selectedPageId ? { width: dom.width, height: dom.height } : selectedSession.viewportSize}
          disabled={!interactive || resizingSelection || awaitingProjection}
          pending={resizingSelection || awaitingProjection}
          readPanelSize={readPanelSize} resize={size => void resizeViewport(size)} /> : null}
        <Button type="button" size="icon-xs" variant="ghost" aria-label={t("browser.refreshBrowserList")} title={t("browser.refreshBrowserList")} onClick={() => {
          // The shared observer reports errors only to the current owner/lifetime.
          // 共享观察者仅向当前归属与生命周期报告错误。
          void refreshSessions().catch(() => {});
        }}>
          <RefreshCw size={13} />
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild><Button type="button" size="icon-xs" variant="ghost" aria-label={t("browser.browserMenu")}><MoreHorizontal size={14} /></Button></DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onSelect={() => setShowRuntime((shown) => !shown)}><Settings2 size={14} />{t("browser.browserComponentStatus")}</DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem disabled={!selectedSession || !selectedPage} onSelect={() => { if (selectedSession && selectedPage) { setCloseError(null); setCloseRequest({ session: selectedSession, page: selectedPage, label: selectedPage.title || browserTabTitle(selectedPage.url) }); } }}><X size={14} />{t("browser.closeCurrentPage")}</DropdownMenuItem>
            <DropdownMenuItem disabled={!selectedSession} onSelect={() => { if (selectedSession) { setCloseError(null); setCloseRequest({ session: selectedSession, page: null, label: selectedPage ? selectedPage.title || browserTabTitle(selectedPage.url) : t("browser.aiBrowser") }); } }} variant="destructive"><X size={14} />{t("browser.closeBrowser")}</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      {selectedSession ? (
        <>
          {error ? <PageNotice tone="error">{error}</PageNotice> : null}
          {domError ? <PageNotice tone="error">{domError}</PageNotice> : null}
          <div ref={viewportPanel} data-browser-viewport className="vk-browser-stage">
            {hasFrame && dom && selectedPage ? (
              <BrowserDomSurface key={`${selectedSessionId}:${dom.pageId}:${dom.documentId}`} snapshot={dom}
                title={t("browser.liveCoreBrowserPage")} interactive={interactive && !resizingSelection && !awaitingProjection}
                heldKeys={heldKeys} heldPointer={heldPointer} queueInput={queueInput} queuePointerMove={queuePointerMove} />
            ) : <span className="vk-browser-wait">{selectedSession.status === "crashed" ? t("browser.viewInterrupted") : t("browser.waitingForBrowserFrame")}</span>}
          </div>
          <BrowserPagePrompts
            dialogs={dialogs}
            fileChoosers={fileChoosers}
            selectedPageId={selectedPageId}
            dialogText={dialogText}
            setDialogText={setDialogText}
            interactive={interactive}
            answerDialog={answerDialog}
            submitFileChooser={submitFileChooser}
          />
          <div className="vk-browser-status">
            <span>{selectedSession.status === "crashed" ? t("browser.viewInterrupted") : streamStatus === "connected" ? t("browser.liveView") : t("browser.connectingView")}</span>
            <Button type="button" size="icon-xs" variant="ghost" title={t("browser.sendTextWithIme")} aria-label={t("browser.textInput")} aria-pressed={showTextInput} onClick={() => setShowTextInput((shown) => !shown)}><Keyboard size={13} /></Button>
            <Button type="button" size="icon-xs" variant="ghost" title={formatMessage(t("browser.downloads"), { length: downloads.length })} aria-label={t("browser.downloadList")} aria-pressed={showDownloads} onClick={() => setShowDownloads((shown) => !shown)}><Download size={13} />{downloads.length ? <span className="vk-browser-count">{downloads.length}</span> : null}</Button>
          </div>
          {showTextInput ? <div className="vk-browser-row">
            <input className="vk-browser-address" value={textInput}
              onChange={(event) => setTextInput(event.target.value)} placeholder={t("browser.typeTextToSend")}
              aria-label={t("browser.textToSendTo")} />
            <Button type="button" size="sm" disabled={!interactive || !hasFrame || !textInput}
              onClick={() => {
                void queueInput("text", { text: textInput }).then(applied => {
                  if (applied) setTextInput(current => current === textInput ? "" : current);
                });
              }}>
              {t("browser.sendText")}
            </Button>
          </div> : null}
          {downloads.length > 0 ? (
            <div hidden={!showDownloads} className="vk-browser-downloads" aria-label={t("browser.browserDownloads")}>
              <div>
              <span>{t("browser.downloads2")}</span>
              {downloads.map((download) => (
                <Button key={download.downloadId} type="button" size="sm" variant="outline"
                  disabled={download.status !== "completed"} onClick={() => void saveDownload(download)}>
                  <Download size={14} />{download.fileName} · {download.status}
                </Button>
              ))}
              </div>
            </div>
          ) : null}
        </>
      ) : <PageEmptyState size="panel" icon={<Globe aria-hidden="true" />} title={sessions.some((session) => session.status === "running")
        ? t("browser.selectABrowserAbove")
        : t("browser.noActiveAiBrowser")} />}
      {!selectedSession && error ? <PageNotice tone="error">{error}</PageNotice> : null}
      {Object.values(pollErrors).some(Boolean) ? <PageNotice tone="error">{Object.values(pollErrors).find(Boolean)}</PageNotice> : null}
      <BrowserCloseConfirmDialog
        closeRequest={closeRequest}
        setCloseRequest={setCloseRequest}
        closePending={closePending}
        closeError={closeError}
        closing={closing}
        closeSession={closeSession}
      />
    </div>
  );
}
