import * as React from "react";
import { ArrowRight, Download, Globe, Hand, Keyboard, MonitorSmartphone, MoreHorizontal, MousePointer2, RefreshCw, Settings2, X } from "lucide-react";
import { PageEmptyState, PageNotice } from "@/components/page";

import { Button } from "@/ui-kit/primitives/button";
import { CoreBrowserRuntimeControls } from "./core-browser-runtime-controls";
import { BrowserDomAssets, type BrowserDomReply, type BrowserDomRevision, type BrowserDomUpdate, type BrowserDomViewport } from "./browser-dom-assets";
import { BrowserDomSurface } from "./browser-dom-surface";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/ui-kit/primitives/dropdown-menu";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { formatMessage, useI18n } from "@/i18n";
import {
  requestCoreBrowser,
  fetchCoreBrowserDownload,
  type CoreBrowserDownload,
  type CoreBrowserDialog,
  type CoreBrowserFileChooser,
  type CoreBrowserLease,
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
  const [showViewport, setShowViewport] = React.useState(false);
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
  // lease holds this UI's in-memory control grant, when acquired.
  // lease 保存本界面获取的内存控制授权。
  const [lease, setLease] = React.useState<CoreBrowserLease | null>(null);
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
  // holderId is this UI instance's stable, nonpersistent control identity.
  // holderId 是本界面实例稳定但不持久化的控制身份。
  const holderId = React.useRef(`webui:${crypto.randomUUID()}`);
  // leaseRef lets unmount cleanup release the exact grant without persisting it.
  // leaseRef 允许卸载清理释放精确授权，而不将其持久化。
  const leaseRef = React.useRef<CoreBrowserLease | null>(null);
  // frameRef gives the input queue the exact generations of the visible DOM viewport.
  // frameRef 为输入队列提供可见 DOM 视口的确切代次。
  const frameRef = React.useRef<BrowserDomViewport | null>(null);
  // requiredFrame blocks input after a page or viewport mutation until a frame from its new generation arrives.
  // requiredFrame 在页面或视口变化后阻止输入，直至新代次画面到达。
  const requiredFrame = React.useRef<{ pageGeneration: string; viewportGeneration: string } | null>(null);
  // projectionSuspended also covers the interval before Core returns the new generation.
  // projectionSuspended 还覆盖 Core 返回新代次之前的时段。
  const projectionSuspended = React.useRef(false);
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

  // selectionIntentSequence prevents a slower tab click from undoing a later tab choice.
  // selectionIntentSequence 防止较慢的页签点击撤销之后的选择。
  const selectionIntentSequence = React.useRef(0);
  // pendingLeaseRelease lets overlapping browser-tab clicks await one exact Core release.
  // pendingLeaseRelease 让重叠的浏览器页签点击等待同一次精确的 Core 授权释放。
  const pendingLeaseRelease = React.useRef<{ ownerSessionId: string; promise: Promise<void> } | null>(null);

  // selectedSession is the latest Core projection for this UI choice.
  // selectedSession 是本界面所选对象的最新 Core 投影。
  const selectedSession = sessions.find((session) => session.sessionId === selectedSessionId) ?? null;
  // selectedPage is the exact Core page currently observed here.
  // selectedPage 是此处观察的精确 Core 页面。
  const selectedPage = pages.find((page) => page.pageId === selectedPageId) ?? null;
  // visibleSessions includes failed browsers so their diagnostics remain reachable.
  // visibleSessions 保留故障浏览器，让诊断信息仍可访问。
  const visibleSessions = sessions.filter((session) => session.status === "running" || session.status === "crashed");
  // ownsControl is true only while this UI retains the selected session's secret.
  // ownsControl 仅在本界面保有选定会话凭据时为真。
  const ownsControl = Boolean(lease && lease.session.sessionId === selectedSessionId);

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
      // previousOwner is the only valid owner for releasing the previous grant.
      // previousOwner 是释放先前授权时唯一有效的归属会话。
      const previousOwner = ownerSessionRef.current;
      // previousLease is captured before the local credential is cleared.
      // previousLease 在本地凭据清除前保存先前授权。
      const previousLease = leaseRef.current;
      leaseRef.current = null;
      pendingLeaseRelease.current = null;
      heldKeys.current.clear();
      heldPointer.current = null;
      pendingPointerMove.current = null;
      selectionIntentSequence.current += 1;
      setLease(null);
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
      if (previousLease) {
        void requestCoreBrowser(previousOwner, "browser.lease.release", {
          sessionId: previousLease.session.sessionId,
          leaseSecret: previousLease.leaseSecret,
        }).catch((failure: unknown) => {
          console.warn("Failed to release previous browser control", failure);
        });
      }
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
    pendingPointerMove.current = null;
    heldPointer.current = null;
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

  React.useEffect(() => {
    if (!lease || !selectedSession) {
      return;
    }
    // A poll started before acquisition may return the older free projection afterward.
    // 取得租约前发出的轮询可能随后才返回旧的空闲投影。
    const observedEpoch = BigInt(selectedSession.controlEpoch);
    const ownedEpoch = BigInt(lease.session.controlEpoch);
    if (selectedSession.status !== "running" || observedEpoch > ownedEpoch
      || (observedEpoch === ownedEpoch && selectedSession.leaseState !== "held")) {
      // A newer Core epoch revokes this UI's stale credential before another input is queued.
      // Core 的新控制代次会在下一条输入排队前撤销本界面的旧凭据。
      leaseRef.current = null;
      setLease(null);
      heldKeys.current.clear();
    }
  }, [lease, selectedSession]);

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
    projectionSuspended.current = false;
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
          "browser.dom.snapshot", { sessionId, pageId, revision: frameRef.current ? revision : null });
        if (!live() || reply.pageId !== pageId || !reply.result) return;
        const required = requiredFrame.current;
        if (projectionSuspended.current && (!required
          || BigInt(reply.pageGeneration) < BigInt(required.pageGeneration)
          || BigInt(reply.viewportGeneration) < BigInt(required.viewportGeneration))) return;
        if ("documentId" in reply.result) {
          const update = reply.result;
          const next = "html" in update ? update : {
            ...update, html: markup.slice(0, update.patch.start) + update.patch.text + markup.slice(update.patch.end),
          };
          const html = await assets.render(owner, sessionId, pageId, next);
          if (!live() || requiredFrame.current !== required) return;
          const viewport = { pageId, pageGeneration: reply.pageGeneration,
            viewportGeneration: reply.viewportGeneration, width: next.width, height: next.height };
          frameRef.current = viewport;
          revision = next.revision;
          markup = next.html;
          projectionSuspended.current = false;
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
    if (!lease || lease.session.ownerSessionId !== props.ownerSessionId) {
      leaseRef.current = null;
      return;
    }
    leaseRef.current = lease;
    // timer renews only this UI's current grant and drops it on an explicit refusal.
    // timer 只续期本界面的当前授权，明确拒绝时立即丢弃。
    const timer = window.setInterval(() => {
      void requestCoreBrowser(props.ownerSessionId, "browser.lease.heartbeat", {
        sessionId: lease.session.sessionId,
        leaseSecret: lease.leaseSecret,
      }).catch((failure: unknown) => {
        if (isCurrentOwner() && leaseRef.current?.leaseSecret === lease.leaseSecret) {
          leaseRef.current = null;
          setLease(null);
          setError(String(failure));
        }
      });
    }, Math.max(1000, lease.heartbeatIntervalMs ?? 5000));
    return () => window.clearInterval(timer);
  }, [lease, props.ownerSessionId]);

  React.useEffect(() => {
    componentMounted.current = true;
    return () => {
      // Closing the GUI releases its lease; the Core browser session remains alive.
      // 关闭 GUI 只释放其租约；Core 浏览器会话继续存活。
      componentMounted.current = false;
      selectionIntentSequence.current += 1;
      const current = leaseRef.current;
      // Revoke local input eligibility before an asynchronous release reaches Core.
      // 异步释放到达 Core 前，先撤销本地输入资格。
      leaseRef.current = null;
      pendingLeaseRelease.current = null;
      selectedSessionRef.current = null;
      selectedPageRef.current = null;
      pendingPointerMove.current = null;
      heldKeys.current.clear();
      heldPointer.current = null;
      if (current) {
        void requestCoreBrowser(ownerSessionRef.current, "browser.lease.release", {
          sessionId: current.session.sessionId,
          leaseSecret: current.leaseSecret,
        }).catch((failure: unknown) => {
          console.warn("Failed to release browser control on unmount", failure);
        });
      }
    };
  }, []);

  // selectSession releases this client's old grant before changing observation.
  // selectSession 在改变观察对象前释放本客户端的旧授权。
  async function selectSession(sessionId: string): Promise<void> {
    if (selectedSessionRef.current === sessionId) return;
    // intent identifies the latest tab choice across overlapping release requests.
    // intent 标识重叠授权释放请求中的最新页签选择。
    const intent = ++selectionIntentSequence.current;
    // previousLease is the credential owned before the tab switch.
    // previousLease 是切换页签前持有的凭据。
    const previousLease = leaseRef.current;
    if (previousLease && previousLease.session.sessionId !== sessionId) {
      // Revoke local input immediately; another tab click must share this release instead of
      // sending the same one-time lease secret to Core twice.
      // 立即撤销本地输入；后续页签点击必须共用释放结果，不能把一次性租约凭据重复发送给 Core。
      leaseRef.current = null;
      pendingPointerMove.current = null;
      heldKeys.current.clear();
      heldPointer.current = null;
      setLease(null);
      pendingLeaseRelease.current = {
        ownerSessionId: props.ownerSessionId,
        promise: requestCoreBrowser(props.ownerSessionId, "browser.lease.release", {
          sessionId: previousLease.session.sessionId,
          leaseSecret: previousLease.leaseSecret,
        }).then(() => undefined),
      };
    }
    const pending = pendingLeaseRelease.current;
    if (pending?.ownerSessionId === props.ownerSessionId) {
      try {
        await pending.promise;
      } catch (failure) {
        if (isCurrentOwner() && intent === selectionIntentSequence.current) setError(String(failure));
        return;
      } finally {
        if (pendingLeaseRelease.current === pending) pendingLeaseRelease.current = null;
      }
    }
    if (!isCurrentOwner() || intent !== selectionIntentSequence.current) return;
    heldKeys.current.clear();
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

  // acquireControl takes a free lease or explicitly takes over another holder.
  // acquireControl 获取空闲租约，或明确接管另一持有者。
  async function acquireControl(): Promise<void> {
    if (!selectedSession) {
      return;
    }
    // sessionId identifies the browser whose lease may still arrive after navigation away.
    // sessionId 标识在界面离开后仍可能返回授权的浏览器。
    const sessionId = selectedSession.sessionId;
    try {
      const method = selectedSession.leaseState === "free" ? "browser.lease.acquire" : "browser.lease.takeover";
      const next = await requestCoreBrowser<CoreBrowserLease>(props.ownerSessionId, method, {
        sessionId,
        holder: holderId.current,
      });
      if (!isCurrentSession(sessionId)) {
        void requestCoreBrowser(props.ownerSessionId, "browser.lease.release", {
          sessionId, leaseSecret: next.leaseSecret,
        }).catch((failure: unknown) => {
          console.warn("Failed to release abandoned browser control", failure);
        });
        return;
      }
      leaseRef.current = next;
      setLease({ ...next, heartbeatIntervalMs: next.heartbeatIntervalMs ?? 5000 });
      setError(null);
      await refreshSessions();
    } catch (failure) {
      if (isCurrentSession(sessionId)) {
        setError(String(failure));
        await refreshSessions().catch((refreshFailure: unknown) => {
          if (isCurrentSession(sessionId)) {
            setPollErrors((current) => ({ ...current, sessions: String(refreshFailure) }));
          }
        });
      }
    }
  }

  // releaseControl gives Core ownership back without closing the page.
  // releaseControl 将控制权交还 Core，而不关闭页面。
  async function releaseControl(): Promise<void> {
    if (!lease) {
      return;
    }
    try {
      await requestCoreBrowser(props.ownerSessionId, "browser.lease.release", {
        sessionId: lease.session.sessionId,
        leaseSecret: lease.leaseSecret,
      });
      if (!isCurrentOwner() || leaseRef.current?.leaseSecret !== lease.leaseSecret) return;
      leaseRef.current = null;
      setLease(null);
      await refreshSessions();
    } catch (failure) {
      if (isCurrentOwner() && leaseRef.current?.leaseSecret === lease.leaseSecret) setError(String(failure));
    }
  }

  // inputQueue serializes pointer, key, wheel and text input behind the current control grant.
  // inputQueue 在当前控制授权下串行化指针、按键、滚轮与文字输入。
  const { queueInput, queuePointerMove } = useBrowserInputQueue({
    leaseRef,
    frameRef,
    projectionSuspended,
    ownerSessionRef,
    selectedSessionRef,
    selectedPageRef,
    pendingPointerMove,
    setError,
    setLease,
  });

  // navigate changes only the selected Core page after a real host confirmation.
  // navigate 仅在真实宿主确认后改变选定的 Core 页面。
  async function navigate(event: React.FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    if (!lease || !selectedSessionId || !selectedPageId) {
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
      const updated = await requestCoreBrowser<CoreBrowserSession>(props.ownerSessionId, "browser.session.navigate", {
        sessionId: selectedSessionId,
        pageId: selectedPageId,
        leaseSecret: lease.leaseSecret,
        url: destination,
      });
      if (!isCurrentPage(selectedSessionId, selectedPageId)) return;
      // Keep the current projection until Core confirms the navigation and supplies a new generation.
      // 保留当前投影，直到 Core 确认导航并提供新代次。
      projectionSuspended.current = true;
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
        setError(String(failure));
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
    if (!lease || lease.session.sessionId !== dialog.sessionId) return;
    try {
      await requestCoreBrowser(props.ownerSessionId, "browser.dialog.respond", {
        sessionId: dialog.sessionId,
        pageId: dialog.pageId,
        dialogId: dialog.dialogId,
        leaseSecret: lease.leaseSecret,
        accept,
        promptText: dialog.dialogType === "prompt" && accept ? dialogText : null,
      });
      if (!isCurrentSession(dialog.sessionId)) return;
      setDialogs((current) => current.filter((item) => item.dialogId !== dialog.dialogId));
      setError(null);
    } catch (failure) {
      if (isCurrentSession(dialog.sessionId)) setError(String(failure));
    }
  }

  // submitFileChooser stages one local file at Core and binds it to the exact clicked input.
  // submitFileChooser 在 Core 暂存一个本地文件，并绑定到精确的已点击输入。
  async function submitFileChooser(chooser: CoreBrowserFileChooser, file: File | undefined): Promise<void> {
    if (!file || !lease || lease.session.sessionId !== chooser.sessionId) return;
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
      if (!isCurrentSession(chooser.sessionId)) return;
      const staged = await requestCoreBrowser<{ uploadId: string }>(props.ownerSessionId, "browser.upload.stage", {
        sessionId: chooser.sessionId, leaseSecret: lease.leaseSecret,
        name: file.name, dataBase64,
      });
      if (!isCurrentSession(chooser.sessionId)) return;
      await requestCoreBrowser(props.ownerSessionId, "browser.upload.attach", {
        sessionId: chooser.sessionId, pageId: chooser.pageId, chooserId: chooser.chooserId,
        leaseSecret: lease.leaseSecret, uploadId: staged.uploadId,
      });
      if (!isCurrentSession(chooser.sessionId)) return;
      setFileChoosers((current) => current.filter((item) => item.chooserId !== chooser.chooserId));
      setError(null);
    } catch (failure) {
      if (isCurrentSession(chooser.sessionId)) setError(String(failure));
    }
  }

  // resizeViewport changes the host content area before Core advances input generations.
  // resizeViewport 在 Core 推进输入代次前调整宿主内容区域。
  async function resizeViewport(value: string): Promise<void> {
    if (!lease || !selectedSessionId) {
      return;
    }
    const [widthText, heightText] = value.split("x");
    projectionSuspended.current = true;
    requiredFrame.current = null;
    frameRef.current = null;
    setDom(null);
    try {
      const updated = await requestCoreBrowser<CoreBrowserSession>(props.ownerSessionId, "browser.viewport.set", {
        sessionId: selectedSessionId,
        leaseSecret: lease.leaseSecret,
        width: Number(widthText),
        height: Number(heightText),
      });
      if (!isCurrentSession(selectedSessionId)) return;
      requiredFrame.current = { pageGeneration: updated.pageGeneration, viewportGeneration: updated.viewportGeneration };
      setSessions((current) => current.map((session) => session.sessionId === updated.sessionId ? updated : session));
      await refreshSessions();
      if (isCurrentSession(selectedSessionId)) setError(null);
    } catch (failure) {
      if (isCurrentSession(selectedSessionId)) {
        projectionSuspended.current = false;
        setError(String(failure));
      }
    }
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
    // closingLease is acquired only for this explicit close when the user is currently observing.
    // closingLease 仅在用户明确关闭且当前只观察时临时获取。
    let closingLease = leaseRef.current?.session.sessionId === sessionId ? leaseRef.current : null;
    let acquiredForClose = false;
    try {
      if (!closingLease) {
        closingLease = await requestCoreBrowser<CoreBrowserLease>(props.ownerSessionId,
          session.leaseState === "free" ? "browser.lease.acquire" : "browser.lease.takeover", {
            sessionId,
            holder: holderId.current,
          });
        acquiredForClose = true;
      }
      if (!isCurrentOwner()) return;
      if (closingLease.session.hostGeneration !== session.hostGeneration) throw new Error("Browser restarted; review the page before closing it.");
      await requestCoreBrowser(props.ownerSessionId, page ? "browser.page.close" : "browser.session.close", {
        sessionId,
        leaseSecret: closingLease.leaseSecret,
        ...(page ? { pageId: page.pageId } : {}),
      });
      if (!isCurrentOwner()) return;
      if (!page) {
        viewedPages.current.delete(sessionId);
        if (isCurrentSession(sessionId)) {
          leaseRef.current = null;
          setLease(null);
          setSelectedSessionId(null);
          setSelectedPageId(null);
        }
      } else if (isCurrentSession(sessionId)) {
        await refreshPages(sessionId);
      }
      await refreshSessions();
      if (isCurrentOwner()) { setError(null); setCloseRequest(null); }
    } catch (failure) {
      if (isCurrentOwner()) setCloseError(String(failure));
    } finally {
      if (acquiredForClose && closingLease) {
        await requestCoreBrowser(props.ownerSessionId, "browser.lease.release", {
          sessionId,
          leaseSecret: closingLease.leaseSecret,
        }).catch(() => undefined);
      }
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
            <Button type="submit" size="icon-sm" variant="ghost" title={t("browser.go")} aria-label={t("browser.go")} disabled={!ownsControl || !selectedPage}><ArrowRight size={14} /></Button>
            <Button type="button" size="icon-sm" variant={ownsControl ? "secondary" : "ghost"}
              disabled={selectedSession.status !== "running" || selectedSession.leaseState === "blocked"}
              title={ownsControl ? t("browser.releaseControl") : selectedSession.leaseState === "free" ? t("browser.takeControl") : t("browser.takeOver")}
              onClick={() => void (ownsControl ? releaseControl() : acquireControl())}>
              {ownsControl ? <MousePointer2 size={14} /> : <Hand size={14} />}<span className="vk-sr-only">{ownsControl ? t("browser.releaseControl") : selectedSession.leaseState === "free" ? t("browser.takeControl") : t("browser.takeOver")}</span>
            </Button>
            <Button type="button" size="icon-sm" variant="ghost" aria-label={t("browser.resizeViewport")} title={t("browser.resizeViewport")} aria-pressed={showViewport} onClick={() => setShowViewport((shown) => !shown)}><MonitorSmartphone size={14} /></Button>
          </form> : <span className="vk-browser-fill" />}
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
          {showViewport ? <div className="vk-browser-size">
            <span>{t("browser.pageSize")}</span>
            <Select disabled={!ownsControl}
              value={selectedSession.viewportSize ? `${selectedSession.viewportSize.width}x${selectedSession.viewportSize.height}` : "1280x800"}
              onValueChange={(value) => void resizeViewport(value)}>
              <SelectTrigger size="sm" aria-label={t("browser.browserViewport")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="1280x800">1280×800</SelectItem>
                <SelectItem value="1024x768">1024×768</SelectItem>
                <SelectItem value="390x844">390×844</SelectItem>
              </SelectContent>
            </Select>
          </div> : null}
          {error ? <PageNotice tone="error">{error}</PageNotice> : null}
          {domError ? <PageNotice tone="error">{domError}</PageNotice> : null}
          <div data-browser-viewport className="vk-browser-stage">
            {hasFrame && dom && selectedPage ? (
              <BrowserDomSurface key={`${selectedSessionId}:${dom.pageId}:${dom.documentId}`} snapshot={dom}
                title={t("browser.liveCoreBrowserPage")} ownsControl={ownsControl}
                heldKeys={heldKeys} heldPointer={heldPointer} queueInput={queueInput} queuePointerMove={queuePointerMove} />
            ) : <span className="vk-browser-wait">{selectedSession.status === "crashed" ? t("browser.viewInterrupted") : t("browser.waitingForBrowserFrame")}</span>}
          </div>
          <BrowserPagePrompts
            dialogs={dialogs}
            fileChoosers={fileChoosers}
            selectedPageId={selectedPageId}
            dialogText={dialogText}
            setDialogText={setDialogText}
            ownsControl={ownsControl}
            answerDialog={answerDialog}
            submitFileChooser={submitFileChooser}
          />
          <div className="vk-browser-status">
            <span>{selectedSession.status === "crashed" ? t("browser.viewInterrupted") : streamStatus === "connected" ? (ownsControl ? t("browser.manualControl") : t("browser.liveView")) : t("browser.connectingView")}</span>
            <Button type="button" size="icon-xs" variant="ghost" title={t("browser.sendTextWithIme")} aria-label={t("browser.textInput")} aria-pressed={showTextInput} onClick={() => setShowTextInput((shown) => !shown)}><Keyboard size={13} /></Button>
            <Button type="button" size="icon-xs" variant="ghost" title={formatMessage(t("browser.downloads"), { length: downloads.length })} aria-label={t("browser.downloadList")} aria-pressed={showDownloads} onClick={() => setShowDownloads((shown) => !shown)}><Download size={13} />{downloads.length ? <span className="vk-browser-count">{downloads.length}</span> : null}</Button>
          </div>
          {showTextInput ? <div className="vk-browser-row">
            <input className="vk-browser-address" value={textInput}
              onChange={(event) => setTextInput(event.target.value)} placeholder={t("browser.typeTextToSend")}
              aria-label={t("browser.textToSendTo")} />
            <Button type="button" size="sm" disabled={!ownsControl || !hasFrame || !textInput}
              onClick={() => { queueInput("text", { text: textInput }); setTextInput(""); }}>
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
        lease={lease}
        closeSession={closeSession}
      />
    </div>
  );
}
