import * as React from "react";
import { Download, Globe2, Plus, RefreshCw, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useI18n } from "@/i18n";
import {
  requestCoreBrowser,
  listCoreBrowserPages,
  listCoreBrowserDownloads,
  listCoreBrowserDialogs,
  listCoreBrowserFileChoosers,
  listCoreBrowserSessions,
  fetchCoreBrowserDownload,
  openCoreBrowserStream,
  type CoreBrowserDownload,
  type CoreBrowserDialog,
  type CoreBrowserFileChooser,
  type CoreBrowserFrame,
  type CoreBrowserLease,
  type CoreBrowserPage,
  type CoreBrowserSession,
} from "@/service/core-browser-service";

// BrowserInputResult describes the Core-confirmed outcome of one input action.
// BrowserInputResult 描述一条输入操作经 Core 确认的结果。
interface BrowserInputResult {
  outcome: "accepted" | "applied" | "rejected" | "unknown";
  reason?: string;
}

// QueuedBrowserInput binds an input to the exact Core owner, browser, page, and control generation.
// QueuedBrowserInput 将输入绑定到精确的 Core 归属、浏览器、页面与控制代次。
interface QueuedBrowserInput {
  ownerSessionId: string;
  sessionId: string;
  pageId: string;
  leaseSecret: string;
  pageGeneration: string;
  viewportGeneration: string;
  kind: string;
  payload: Record<string, unknown>;
}

// HeldBrowserPointer remembers the pressed button and last page coordinates until release.
// HeldBrowserPointer 记录已按下按钮及最后页面坐标，直至指针释放。
interface HeldBrowserPointer {
  pointerId: number;
  button: "left" | "middle" | "right";
  x: number;
  y: number;
}

// BrowserPollSource identifies independently refreshed Core browser metadata.
// BrowserPollSource 标识独立刷新的 Core 浏览器元数据来源。
type BrowserPollSource = "sessions" | "pages" | "downloads" | "fileChoosers" | "dialogs";

// BrowserArea lets any WebUI client observe and control a Core-owned browser page.
// BrowserArea 让任意 WebUI 客户端观察和控制 Core 拥有的浏览器页面。
export function BrowserArea(props: { ownerSessionId: string }) {
  // locale selects labels without changing the Core wire protocol.
  // locale 选择文案而不改变 Core 线协议。
  const { locale } = useI18n();
  // chinese determines the browser panel's visible language.
  // chinese 决定浏览器面板的显示语言。
  const chinese = locale === "zh-CN";
  // sessions is the latest Core-owned session list.
  // sessions 是最新的 Core 自有会话列表。
  const [sessions, setSessions] = React.useState<CoreBrowserSession[]>([]);
  // selectedSessionId is this client's UI selection, independent of AI ownership.
  // selectedSessionId 是本客户端的界面选择，与 AI 归属无关。
  const [selectedSessionId, setSelectedSessionId] = React.useState<string | null>(null);
  // pages is the selected session's current page registry.
  // pages 是选定会话当前的页面注册表。
  const [pages, setPages] = React.useState<CoreBrowserPage[]>([]);
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
  // lease holds this UI's in-memory control grant, when acquired.
  // lease 保存本界面获取的内存控制授权。
  const [lease, setLease] = React.useState<CoreBrowserLease | null>(null);
  // address is the editable navigation destination.
  // address 是可编辑的导航目标地址。
  const [address, setAddress] = React.useState("about:blank");
  // textInput is committed user text sent to the browser page, including IME text.
  // textInput 是发送到浏览器页面的用户已提交文本，包括输入法文本。
  const [textInput, setTextInput] = React.useState("");
  // imageUrl points to the latest binary frame rendered by this observer.
  // imageUrl 指向本观察端渲染的最新二进制帧。
  const [imageUrl, setImageUrl] = React.useState<string | null>(null);
  // frame records the page and viewport generations that authorize new input.
  // frame 记录授权新输入的页面和视口代次。
  const [frame, setFrame] = React.useState<CoreBrowserFrame | null>(null);
  // streamStatus names the current observation connection state.
  // streamStatus 命名当前观察连接状态。
  const [streamStatus, setStreamStatus] = React.useState("disconnected");
  // error keeps a refused or uncertain operation visible to the user.
  // error 让被拒绝或结果未知的操作保持对用户可见。
  const [error, setError] = React.useState<string | null>(null);
  // pollErrors keep recoverable list failures separate from explicit operation results.
  // pollErrors 将可恢复的列表失败与显式操作结果分开保存。
  const [pollErrors, setPollErrors] = React.useState<Record<BrowserPollSource, string | null>>({
    sessions: null, pages: null, downloads: null, fileChoosers: null, dialogs: null,
  });
  // busy disables repeated session or page creation while Core is responding.
  // busy 在 Core 应答期间禁止重复创建会话或页面。
  const [busy, setBusy] = React.useState(false);
  // holderId is this UI instance's stable, nonpersistent control identity.
  // holderId 是本界面实例稳定但不持久化的控制身份。
  const holderId = React.useRef(`webui:${crypto.randomUUID()}`);
  // leaseRef lets unmount cleanup release the exact grant without persisting it.
  // leaseRef 允许卸载清理释放精确授权，而不将其持久化。
  const leaseRef = React.useRef<CoreBrowserLease | null>(null);
  // imageUrlRef revokes the previous Blob URL whenever a fresh frame arrives.
  // imageUrlRef 在新帧到达时回收先前的 Blob URL。
  const imageUrlRef = React.useRef<string | null>(null);
  // frameRef gives pointer handlers the newest measured image geometry.
  // frameRef 为指针处理器提供最新测量的图像尺寸。
  const frameRef = React.useRef<CoreBrowserFrame | null>(null);
  // requiredFrame blocks input after a page or viewport mutation until a frame from its new generation arrives.
  // requiredFrame 在页面或视口变化后阻止输入，直至新代次画面到达。
  const requiredFrame = React.useRef<{ pageGeneration: string; viewportGeneration: string } | null>(null);
  // projectionSuspended also covers the interval before Core returns the new generation.
  // projectionSuspended 还覆盖 Core 返回新代次之前的时段。
  const projectionSuspended = React.useRef(false);
  // inputSequence tracks the sender sequence required by Core's input protocol.
  // inputSequence 跟踪 Core 输入协议要求的发送方序号。
  const inputSequence = React.useRef(0);
  // inputQueue preserves down/up order across asynchronous HTTP requests.
  // inputQueue 在异步 HTTP 请求之间保持按下和释放的顺序。
  const inputQueue = React.useRef<Promise<void>>(Promise.resolve());
  // pendingPointerMove retains only the latest not-yet-dispatched drag position.
  // pendingPointerMove 仅保留尚未派发的最新拖拽位置。
  const pendingPointerMove = React.useRef<QueuedBrowserInput | null>(null);
  // pointerMoveQueued ensures at most one coalescing worker waits in the input queue.
  // pointerMoveQueued 确保输入队列中最多只有一个移动事件合并任务。
  const pointerMoveQueued = React.useRef(false);
  // heldKeys lets focus loss release only keys this view actually pressed.
  // heldKeys 使失焦时只释放此视图确实按下过的按键。
  const heldKeys = React.useRef(new Set<string>());
  // heldPointer lets a drag finish after the pointer leaves the projected image.
  // heldPointer 允许指针离开投影画面后仍完成拖拽释放。
  const heldPointer = React.useRef<HeldBrowserPointer | null>(null);
  // addressRef prevents background page refresh from replacing text during editing.
  // addressRef 防止后台页面刷新覆盖正在编辑的地址。
  const addressRef = React.useRef<HTMLInputElement | null>(null);
  // projectionRef binds a non-passive wheel listener to the exact visible Core frame.
  // projectionRef 将非被动滚轮监听器绑定到当前可见的 Core 画面。
  const projectionRef = React.useRef<HTMLDivElement | null>(null);
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
  // sessionRequestSequence orders overlapping browser-list responses.
  // sessionRequestSequence 为重叠的浏览器列表响应排序。
  const sessionRequestSequence = React.useRef(0);
  // appliedSessionSequence records the newest browser-list response already shown.
  // appliedSessionSequence 记录已经展示的最新浏览器列表响应。
  const appliedSessionSequence = React.useRef(0);
  // failedSessionSequence prevents an older failure from replacing a newer failure.
  // failedSessionSequence 防止较旧失败覆盖较新的失败。
  const failedSessionSequence = React.useRef(0);
  // pageRequestSequence orders overlapping page-list responses.
  // pageRequestSequence 为重叠的页面列表响应排序。
  const pageRequestSequence = React.useRef(0);
  // appliedPageSequence records the newest page-list response already shown.
  // appliedPageSequence 记录已经展示的最新页面列表响应。
  const appliedPageSequence = React.useRef(0);
  // failedPageSequence prevents an older page-list failure from replacing a newer one.
  // failedPageSequence 防止较旧的页面列表失败覆盖较新的失败。
  const failedPageSequence = React.useRef(0);
  // selectionIntentSequence prevents a slower tab click from undoing a later tab choice.
  // selectionIntentSequence 防止较慢的页签点击撤销之后的选择。
  const selectionIntentSequence = React.useRef(0);
  // pendingLeaseRelease lets overlapping browser-tab clicks await one exact Core release.
  // pendingLeaseRelease 让重叠的浏览器页签点击等待同一次精确的 Core 授权释放。
  const pendingLeaseRelease = React.useRef<{ ownerSessionId: string; promise: Promise<void> } | null>(null);
  // busyOperationSequence prevents an abandoned operation from clearing a newer busy state.
  // busyOperationSequence 防止被放弃的操作清除后续操作的忙碌状态。
  const busyOperationSequence = React.useRef(0);

  // selectedSession is the latest Core projection for this UI choice.
  // selectedSession 是本界面所选对象的最新 Core 投影。
  const selectedSession = sessions.find((session) => session.sessionId === selectedSessionId) ?? null;
  // selectedPage is the exact Core page currently observed here.
  // selectedPage 是此处观察的精确 Core 页面。
  const selectedPage = pages.find((page) => page.pageId === selectedPageId) ?? null;
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
      busyOperationSequence.current += 1;
      setLease(null);
      setBusy(false);
      setSessions([]);
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
    // Clear the old browser before paint so a changed selection never displays its metadata.
    // 在绘制前清空旧浏览器，避免切换选择后显示其元数据。
    busyOperationSequence.current += 1;
    setBusy(false);
    pendingPointerMove.current = null;
    heldPointer.current = null;
    setPages([]);
    setDownloads([]);
    setDialogs([]);
    setFileChoosers([]);
    setPollErrors({ sessions: null, pages: null, downloads: null, fileChoosers: null, dialogs: null });
    setSelectedPageId(null);
  }, [props.ownerSessionId, selectedSessionId]);

  React.useLayoutEffect(() => {
    // A page switch must hide the previous frame before its old stream cleanup runs.
    // 页面切换时须在旧流清理运行前隐藏上一页画面。
    if (imageUrlRef.current) URL.revokeObjectURL(imageUrlRef.current);
    imageUrlRef.current = null;
    frameRef.current = null;
    setImageUrl(null);
    setFrame(null);
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

  // refreshSessions adopts Core's session list without selecting an unrelated browser.
  // refreshSessions 采用 Core 会话列表，但不会选择无关浏览器。
  const refreshSessions = React.useCallback(async () => {
    const requestSequence = ++sessionRequestSequence.current;
    const next = await listCoreBrowserSessions(props.ownerSessionId);
    if (ownerSessionRef.current !== props.ownerSessionId
      || requestSequence <= appliedSessionSequence.current
      || requestSequence <= failedSessionSequence.current) return;
    appliedSessionSequence.current = requestSequence;
    setSessions(next);
    setPollErrors((current) => current.sessions === null ? current : { ...current, sessions: null });
    setSelectedSessionId((current) => current && next.some((session) => session.sessionId === current)
      ? current : next.find((session) => session.status === "running")?.sessionId ?? null);
  }, [props.ownerSessionId]);

  // refreshPages updates one selected session's page identity list.
  // refreshPages 更新一个选定会话的页面身份列表。
  const refreshPages = React.useCallback(async (sessionId: string) => {
    const requestSequence = ++pageRequestSequence.current;
    const next = await listCoreBrowserPages(props.ownerSessionId, sessionId);
    if (ownerSessionRef.current !== props.ownerSessionId || selectedSessionRef.current !== sessionId
      || requestSequence <= appliedPageSequence.current
      || requestSequence <= failedPageSequence.current) return;
    appliedPageSequence.current = requestSequence;
    setPages(next);
    setPollErrors((current) => current.pages === null ? current : { ...current, pages: null });
    setSelectedPageId((current) => current && next.some((page) => page.pageId === current)
      ? current : next[0]?.pageId ?? null);
  }, [props.ownerSessionId]);

  React.useEffect(() => {
    // timer periodically discovers popups, page closure, and control handoff from Core.
    // timer 定期从 Core 发现弹出页、页面关闭与控制权交接。
    // cancelled prevents an old conversation poll from publishing an error after navigation.
    // cancelled 阻止旧会话轮询在导航后发布错误。
    let cancelled = false;
    // refresh reports only failures belonging to the current conversation.
    // refresh 只报告当前会话的失败。
    const refresh = () => {
      // A failed older poll must not replace a newer successful browser list with a stale error.
      // 旧轮询失败不得以过时错误覆盖较新的浏览器列表成功结果。
      // requestSequence identifies this issued browser-list request for error ordering.
      // requestSequence 标识本次已发出的浏览器列表请求，用于错误排序。
      const requestSequence = sessionRequestSequence.current + 1;
      void refreshSessions().catch((failure: unknown) => {
        if (!cancelled && ownerSessionRef.current === props.ownerSessionId
          && requestSequence > appliedSessionSequence.current
          && requestSequence > failedSessionSequence.current) {
          failedSessionSequence.current = requestSequence;
          setPollErrors((current) => ({ ...current, sessions: String(failure) }));
        }
      });
    };
    refresh();
    // timer owns only this conversation's browser-list polling interval.
    // timer 只拥有此会话浏览器列表的轮询间隔。
    const timer = window.setInterval(refresh, 1500);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [props.ownerSessionId, refreshSessions]);

  React.useEffect(() => {
    if (!selectedSessionId) {
      return;
    }
    // timer keeps this client's page list aligned with Core-owned popups.
    // timer 使本客户端的页面列表与 Core 拥有的弹出页保持一致。
    // cancelled prevents a prior browser's late error from replacing the current state.
    // cancelled 阻止先前浏览器的迟到错误覆盖当前状态。
    let cancelled = false;
    // refresh reports failures only while this browser remains selected.
    // refresh 仅在此浏览器仍被选中时报告失败。
    const refresh = () => {
      // Ignore a previous page poll's error after a newer selection or response has won.
      // 较新的页面选择或响应生效后忽略先前轮询的错误。
      // requestSequence identifies this issued page-list request for error ordering.
      // requestSequence 标识本次已发出的页面列表请求，用于错误排序。
      const requestSequence = pageRequestSequence.current + 1;
      void refreshPages(selectedSessionId).catch((failure: unknown) => {
        if (!cancelled && ownerSessionRef.current === props.ownerSessionId
          && selectedSessionRef.current === selectedSessionId
          && requestSequence > appliedPageSequence.current
          && requestSequence > failedPageSequence.current) {
          failedPageSequence.current = requestSequence;
          setPollErrors((current) => ({ ...current, pages: String(failure) }));
        }
      });
    };
    refresh();
    // timer owns the page-list interval for this exact browser selection.
    // timer 拥有此精确浏览器选择的页面列表轮询间隔。
    const timer = window.setInterval(refresh, 1500);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [props.ownerSessionId, refreshPages, selectedSessionId]);

  React.useEffect(() => {
    if (!selectedSessionId) return;
    // cancelled and the owner/selection checks reject responses from a previous panel lifetime.
    // cancelled 与归属、选择检查共同拒绝上一次面板生命周期的迟到响应。
    let cancelled = false;
    // makeRefresh orders one metadata source without blocking its polling interval.
    // makeRefresh 对单个元数据来源排序，同时不阻塞其轮询间隔。
    // load reads one Core list; apply publishes only the newest response; returns a poll callback.
    // load 读取一份 Core 列表；apply 仅发布最新响应；返回轮询回调。
    const makeRefresh = <T,>(source: BrowserPollSource, load: () => Promise<T[]>, apply: React.Dispatch<React.SetStateAction<T[]>>) => {
      // issued is the latest started request for this metadata source.
      // issued 是此元数据来源最新发出的请求序号。
      let issued = 0;
      // applied is the latest response already published for this metadata source.
      // applied 是此元数据来源已经发布的最新响应序号。
      let applied = 0;
      // failed is the newest failure published for this metadata source.
      // failed 是此元数据来源已发布的最新失败序号。
      let failed = 0;
      return () => {
        // sequence preserves request order even when network responses arrive out of order.
        // sequence 在网络响应乱序到达时保留请求顺序。
        const sequence = ++issued;
        void load().then((next) => {
          if (cancelled || ownerSessionRef.current !== props.ownerSessionId
            || selectedSessionRef.current !== selectedSessionId
            || sequence <= applied || sequence <= failed) return;
          applied = sequence;
          apply(next);
          setPollErrors((current) => current[source] === null ? current : { ...current, [source]: null });
        }).catch((failure: unknown) => {
          if (!cancelled && ownerSessionRef.current === props.ownerSessionId
            && selectedSessionRef.current === selectedSessionId
            && sequence > applied && sequence > failed) {
            failed = sequence;
            setPollErrors((current) => ({ ...current, [source]: String(failure) }));
          }
        });
      };
    };
    // Only metadata is polled; download bytes still require explicit user action.
    // 轮询仅读取元数据；下载文件内容仍须用户明确操作。
    const refreshDownloads = makeRefresh(
      "downloads",
      () => listCoreBrowserDownloads(props.ownerSessionId, selectedSessionId), setDownloads,
    );
    // File choosers and dialogs refresh more often because they block page interaction.
    // 文件选择器与弹窗会阻塞页面交互，因此采用更短的刷新间隔。
    const refreshFileChoosers = makeRefresh(
      "fileChoosers",
      () => listCoreBrowserFileChoosers(props.ownerSessionId, selectedSessionId), setFileChoosers,
    );
    const refreshDialogs = makeRefresh(
      "dialogs",
      () => listCoreBrowserDialogs(props.ownerSessionId, selectedSessionId), setDialogs,
    );
    refreshDownloads();
    refreshFileChoosers();
    refreshDialogs();
    // downloadTimer, chooserTimer and dialogTimer belong to this exact browser selection.
    // downloadTimer、chooserTimer 和 dialogTimer 仅属于此浏览器选择。
    const downloadTimer = window.setInterval(refreshDownloads, 1500);
    const chooserTimer = window.setInterval(refreshFileChoosers, 500);
    const dialogTimer = window.setInterval(refreshDialogs, 500);
    return () => {
      cancelled = true;
      window.clearInterval(downloadTimer);
      window.clearInterval(chooserTimer);
      window.clearInterval(dialogTimer);
    };
  }, [props.ownerSessionId, selectedSessionId]);

  React.useEffect(() => {
    setDialogText(dialogs.find((dialog) => dialog.pageId === selectedPageId)?.defaultPrompt ?? "");
  }, [dialogs.map((dialog) => dialog.dialogId).join("|"), selectedPageId]);

  React.useEffect(() => {
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
    // cancelled prevents a late ticket response from reopening an abandoned page stream.
    // cancelled 防止迟到的凭据应答重新打开已放弃的页面流。
    let cancelled = false;
    // close owns the exact socket created for this selection.
    // close 拥有为本次选择创建的精确套接字。
    let close: (() => void) | null = null;
    // reconnectTimer is cleared when this page selection changes.
    // reconnectTimer 在页面选择改变时被清除。
    let reconnectTimer: number | null = null;
    // connect establishes a fresh one-use stream ticket after each disconnect.
    // connect 在每次断连后以新的一次性凭据建立流。
    const connect = async () => {
      try {
        close = await openCoreBrowserStream(props.ownerSessionId, selectedSessionId, selectedPageId, {
          onFrame: (nextFrame) => {
            if (cancelled || ownerSessionRef.current !== props.ownerSessionId
              || selectedSessionRef.current !== selectedSessionId
              || selectedPageRef.current !== selectedPageId || nextFrame.pageId !== selectedPageId) {
              return;
            }
            if (projectionSuspended.current) {
              const required = requiredFrame.current;
              if (!required || BigInt(nextFrame.pageGeneration) < BigInt(required.pageGeneration)
                || BigInt(nextFrame.viewportGeneration) < BigInt(required.viewportGeneration)) return;
              projectionSuspended.current = false;
              requiredFrame.current = null;
            }
            // blobUrl holds one browser-renderable frame without exposing the page's origin.
            // blobUrl 保存浏览器可渲染的一帧，而不暴露页面源站。
            const blobUrl = URL.createObjectURL(new Blob([nextFrame.image], { type: nextFrame.mimeType }));
            if (imageUrlRef.current) {
              URL.revokeObjectURL(imageUrlRef.current);
            }
            imageUrlRef.current = blobUrl;
            frameRef.current = nextFrame;
            setFrame(nextFrame);
            setImageUrl(blobUrl);
          },
          onState: (status) => {
            if (cancelled || ownerSessionRef.current !== props.ownerSessionId
              || selectedSessionRef.current !== selectedSessionId
              || selectedPageRef.current !== selectedPageId) {
              return;
            }
            setStreamStatus(status);
            if (status === "disconnected" && reconnectTimer === null) {
              reconnectTimer = window.setTimeout(() => {
                reconnectTimer = null;
                void connect();
              }, 1000);
            }
          },
          onError: (message) => {
            if (!cancelled && ownerSessionRef.current === props.ownerSessionId
              && selectedSessionRef.current === selectedSessionId
              && selectedPageRef.current === selectedPageId) {
              setError(message);
            }
          },
        });
        if (cancelled || ownerSessionRef.current !== props.ownerSessionId
          || selectedSessionRef.current !== selectedSessionId
          || selectedPageRef.current !== selectedPageId) {
          close();
        }
      } catch (failure) {
        if (!cancelled && ownerSessionRef.current === props.ownerSessionId
          && selectedSessionRef.current === selectedSessionId
          && selectedPageRef.current === selectedPageId) {
          setError(String(failure));
          reconnectTimer = window.setTimeout(() => {
            reconnectTimer = null;
            void connect();
          }, 1000);
        }
      }
    };
    setStreamStatus("connecting");
    void connect();
    return () => {
      cancelled = true;
      close?.();
      if (reconnectTimer !== null) {
        window.clearTimeout(reconnectTimer);
      }
      if (imageUrlRef.current) {
        URL.revokeObjectURL(imageUrlRef.current);
        imageUrlRef.current = null;
      }
      frameRef.current = null;
      setImageUrl(null);
      setFrame(null);
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
      busyOperationSequence.current += 1;
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

  // createSession asks Core to start a new background host and returns its stable identity.
  // createSession 请求 Core 启动新后台宿主并返回其稳定身份。
  async function createSession(): Promise<void> {
    // operation prevents an old conversation's completion from changing this panel's busy state.
    // operation 防止旧会话的完成结果修改当前面板的忙碌状态。
    const operation = ++busyOperationSequence.current;
    // selectionIntent preserves an explicit tab choice made while creation is pending.
    // selectionIntent 保留建会话等待期间用户明确做出的页签选择。
    const selectionIntent = selectionIntentSequence.current;
    setBusy(true);
    setError(null);
    try {
      const created = await requestCoreBrowser<CoreBrowserSession>(props.ownerSessionId, "browser.session.create", { profileMode: "ephemeral" });
      if (!isCurrentOwner()) return;
      await refreshSessions();
      if (!isCurrentOwner()) return;
      if (selectionIntent === selectionIntentSequence.current) await selectSession(created.sessionId);
    } catch (failure) {
      if (isCurrentOwner()) setError(String(failure));
    } finally {
      if (isCurrentOwner() && operation === busyOperationSequence.current) setBusy(false);
    }
  }

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

  // captureInput freezes the current owner, page, lease, and frame for one input event.
  // captureInput 为一条输入固定当前归属、页面、租约和画面；无可用授权时返回空值。
  function captureInput(kind: string, payload: Record<string, unknown>): QueuedBrowserInput | null {
    const currentLease = leaseRef.current;
    const currentFrame = frameRef.current;
    const sessionId = selectedSessionRef.current;
    const pageId = selectedPageRef.current;
    if (!currentLease || !currentFrame || projectionSuspended.current || !sessionId || !pageId
      || currentLease.session.sessionId !== sessionId || currentFrame.pageId !== pageId) return null;
    return {
      ownerSessionId: ownerSessionRef.current, sessionId, pageId,
      leaseSecret: currentLease.leaseSecret,
      pageGeneration: currentFrame.pageGeneration,
      viewportGeneration: currentFrame.viewportGeneration,
      kind, payload,
    };
  }

  // inputStillAuthorized permits only key or pointer release to finish on a previously selected page.
  // inputStillAuthorized 只允许按键或指针释放在先前选中的页面完成；返回当前授权是否仍有效。
  function inputStillAuthorized(input: QueuedBrowserInput): boolean {
    // releasingInput never authorizes an old conversation or an old browser lease.
    // releasingInput 不会授权旧会话或旧浏览器租约。
    const releasingInput = (input.kind === "key" || input.kind === "pointer") && input.payload.type === "up";
    return ownerSessionRef.current === input.ownerSessionId
      && selectedSessionRef.current === input.sessionId
      && leaseRef.current?.leaseSecret === input.leaseSecret
      && (releasingInput || selectedPageRef.current === input.pageId);
  }

  // deliverInput sends one captured event in queue order and publishes only its still-current outcome.
  // deliverInput 按队列顺序发送一条固定的输入，并仅发布仍属当前选择的结果。
  async function deliverInput(input: QueuedBrowserInput): Promise<void> {
    if (!inputStillAuthorized(input)) return;
    // sequence is assigned at dispatch time so coalesced moves leave no sender gaps.
    // sequence 在实际派发时分配，使合并的移动事件不留下发送方序号空洞。
    const sequence = String(++inputSequence.current);
    try {
      const result = await requestCoreBrowser<BrowserInputResult>(input.ownerSessionId, "browser.input.dispatch", {
        sessionId: input.sessionId,
        pageId: input.pageId,
        leaseSecret: input.leaseSecret,
        pageGeneration: input.pageGeneration,
        viewportGeneration: input.viewportGeneration,
        sequence,
        kind: input.kind,
        payload: input.payload,
      });
      if (result.outcome !== "applied" && inputStillAuthorized(input)) {
        setError(result.reason ?? `Input ${result.outcome}`);
        if (result.outcome === "unknown" || result.reason === "lease_required" || result.reason === "lease_blocked") {
          leaseRef.current = null;
          setLease(null);
        }
      }
    } catch (failure) {
      if (inputStillAuthorized(input)) setError(String(failure));
      // Transport failure does not prove lease loss; Core will reject stale credentials.
      // 传输失败不能证明租约已丢失；过期凭据会由 Core 明确拒绝。
    }
  }

  // queueCapturedInput appends one exact event behind all earlier browser inputs.
  // queueCapturedInput 将一条精确输入追加在此前所有浏览器输入之后。
  function queueCapturedInput(input: QueuedBrowserInput): void {
    inputQueue.current = inputQueue.current.then(() => deliverInput(input));
  }

  // flushPendingPointerMove schedules one latest drag position and repeats only when a newer one exists.
  // flushPendingPointerMove 调度一个最新拖拽位置，只有出现更新位置时才继续调度。
  function flushPendingPointerMove(): void {
    if (pointerMoveQueued.current || !pendingPointerMove.current) return;
    pointerMoveQueued.current = true;
    inputQueue.current = inputQueue.current.then(async () => {
      // latest replaces any earlier move that has not entered Core's dispatch.
      // latest 替换尚未进入 Core 派发的所有更早移动事件。
      const latest = pendingPointerMove.current;
      pendingPointerMove.current = null;
      if (latest) await deliverInput(latest);
    }).finally(() => {
      pointerMoveQueued.current = false;
      if (pendingPointerMove.current) flushPendingPointerMove();
    });
  }

  // queuePointerMove keeps one pending drag point instead of accumulating one HTTP request per event.
  // queuePointerMove 仅保留一个待派发拖拽位置，避免为每次指针移动累积 HTTP 请求。
  function queuePointerMove(payload: Record<string, unknown>): void {
    const input = captureInput("pointer", payload);
    if (!input) return;
    pendingPointerMove.current = input;
    flushPendingPointerMove();
  }

  // queueInput preserves down/up, key, text, and wheel order around coalesced pointer moves.
  // queueInput 在合并指针移动的同时保持按下、抬起、按键、文字和滚轮的顺序。
  function queueInput(kind: string, payload: Record<string, unknown>): void {
    // An intervening non-move event makes the last pending move observable before that event.
    // 插入的非移动事件必须排在最后一个待派发移动事件之后。
    const pending = pendingPointerMove.current;
    if (pending) {
      pendingPointerMove.current = null;
      queueCapturedInput(pending);
    }
    const input = captureInput(kind, payload);
    if (input) queueCapturedInput(input);
  }

  // pointerCoordinates maps a pointer or wheel event through the scaled frame to CSS page pixels.
  // pointerCoordinates 将指针或滚轮事件经缩放画面换算为页面 CSS 像素；无有效画面时返回空值。
  function pointerCoordinates(event: { currentTarget: HTMLDivElement; clientX: number; clientY: number }): { x: number; y: number } | null {
    const currentFrame = frameRef.current;
    if (!currentFrame) {
      return null;
    }
    const bounds = event.currentTarget.getBoundingClientRect();
    const scale = Math.min(bounds.width / currentFrame.width, bounds.height / currentFrame.height);
    if (!Number.isFinite(scale) || scale <= 0) return null;
    const left = (bounds.width - currentFrame.width * scale) / 2;
    const top = (bounds.height - currentFrame.height * scale) / 2;
    return {
      x: (event.clientX - bounds.left - left) / scale,
      y: (event.clientY - bounds.top - top) / scale,
    };
  }

  // pressPointer captures the primary pointer and sends its first page-coordinate down event.
  // pressPointer 捕获主指针，并向当前页面发送首条页面坐标的按下事件。
  function pressPointer(event: React.PointerEvent<HTMLDivElement>): void {
    if (!ownsControl || !event.isPrimary || heldPointer.current) return;
    event.preventDefault();
    event.currentTarget.focus();
    const coordinates = pointerCoordinates(event);
    if (coordinates) {
      // button retains the initiating button even when pointerup reports a different value.
      // button 保留起始按钮，即使 pointerup 报告不同按钮值也不会改变。
      const button = event.button === 2 ? "right" : event.button === 1 ? "middle" : "left";
      heldPointer.current = { pointerId: event.pointerId, button, ...coordinates };
      event.currentTarget.setPointerCapture(event.pointerId);
      queueInput("pointer", { type: "down", button, ...coordinates });
    }
  }

  // movePointer coalesces drag positions while retaining the pressed-button bit field for CDP.
  // movePointer 合并拖拽位置，并为 CDP 保留已按下按钮的位字段。
  function movePointer(event: React.PointerEvent<HTMLDivElement>): void {
    const held = heldPointer.current;
    if (!ownsControl || !held || held.pointerId !== event.pointerId) return;
    event.preventDefault();
    const coordinates = pointerCoordinates(event);
    if (!coordinates) return;
    heldPointer.current = { ...held, ...coordinates };
    const buttons = held.button === "right" ? 2 : held.button === "middle" ? 4 : 1;
    queuePointerMove({ type: "move", buttons, ...coordinates });
  }

  // releasePointer sends the matching up event after the latest pending drag position.
  // releasePointer 在最后一个待派发拖拽位置之后发送对应的抬起事件。
  function releasePointer(event: React.PointerEvent<HTMLDivElement>): void {
    const held = heldPointer.current;
    if (!held || held.pointerId !== event.pointerId) return;
    heldPointer.current = null;
    const coordinates = pointerCoordinates(event) ?? { x: held.x, y: held.y };
    queueInput("pointer", { type: "up", button: held.button, ...coordinates });
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  }

  React.useLayoutEffect(() => {
    // React's delegated wheel listener is passive in Chromium; bind this page input non-passively.
    // Chromium 中 React 委托的滚轮监听器为被动模式；此页面输入须以非被动方式绑定。
    const element = projectionRef.current;
    if (!element || !ownsControl) return;
    // handleWheel prevents workbench scrolling before Core receives page-relative wheel coordinates.
    // handleWheel 在 Core 接收页面相对坐标前阻止工作台滚动。
    const handleWheel = (event: WheelEvent) => {
      const coordinates = pointerCoordinates({ currentTarget: element, clientX: event.clientX, clientY: event.clientY });
      if (!coordinates) return;
      event.preventDefault();
      // factor converts DOM line/page wheel units to the host's CSS-pixel input contract.
      // factor 将 DOM 行/页滚轮单位换算为宿主输入协议使用的 CSS 像素。
      const factor = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? frameRef.current?.height ?? 800 : 1;
      queueInput("wheel", { ...coordinates, deltaX: event.deltaX * factor, deltaY: event.deltaY * factor });
    };
    element.addEventListener("wheel", handleWheel, { passive: false });
    return () => element.removeEventListener("wheel", handleWheel);
  }, [props.ownerSessionId, selectedSessionId, selectedPageId, ownsControl, Boolean(imageUrl)]);

  // navigate changes only the selected Core page after a real host confirmation.
  // navigate 仅在真实宿主确认后改变选定的 Core 页面。
  async function navigate(event: React.FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    if (!lease || !selectedSessionId || !selectedPageId) {
      return;
    }
    projectionSuspended.current = true;
    requiredFrame.current = null;
    frameRef.current = null;
    setFrame(null);
    setImageUrl(null);
    if (imageUrlRef.current) URL.revokeObjectURL(imageUrlRef.current);
    imageUrlRef.current = null;
    try {
      const updated = await requestCoreBrowser<CoreBrowserSession>(props.ownerSessionId, "browser.session.navigate", {
        sessionId: selectedSessionId,
        pageId: selectedPageId,
        leaseSecret: lease.leaseSecret,
        url: address,
      });
      if (!isCurrentPage(selectedSessionId, selectedPageId)) return;
      requiredFrame.current = { pageGeneration: updated.pageGeneration, viewportGeneration: updated.viewportGeneration };
      setSessions((current) => current.map((session) => session.sessionId === updated.sessionId ? updated : session));
      await refreshPages(selectedSessionId);
      if (!isCurrentPage(selectedSessionId, selectedPageId)) return;
      await refreshSessions();
      if (isCurrentPage(selectedSessionId, selectedPageId)) setError(null);
    } catch (failure) {
      if (isCurrentPage(selectedSessionId, selectedPageId)) {
        projectionSuspended.current = false;
        setError(String(failure));
      }
    }
  }

  // createPage asks the current host to add one page under the current lease.
  // createPage 请求当前宿主在当前租约下新增一个页面。
  async function createPage(): Promise<void> {
    if (!lease || !selectedSessionId) {
      return;
    }
    // operation prevents a previous page creation from clearing a later busy state.
    // operation 防止先前建页操作清除较新操作的忙碌状态。
    const operation = ++busyOperationSequence.current;
    setBusy(true);
    try {
      const existing = new Set(pages.map((page) => page.pageId));
      const updated = await requestCoreBrowser<CoreBrowserPage[]>(props.ownerSessionId, "browser.page.create", {
        sessionId: selectedSessionId,
        leaseSecret: lease.leaseSecret,
        url: "about:blank",
      });
      if (!isCurrentSession(selectedSessionId)) return;
      setPages(updated);
      setSelectedPageId(updated.find((page) => !existing.has(page.pageId))?.pageId ?? selectedPageId);
      setError(null);
    } catch (failure) {
      if (isCurrentSession(selectedSessionId)) setError(String(failure));
    } finally {
      if (isCurrentSession(selectedSessionId) && operation === busyOperationSequence.current) setBusy(false);
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
      setError(chinese ? "单个文件暂时不能超过 8 MiB。" : "One file cannot exceed 8 MiB yet.");
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
    setFrame(null);
    setImageUrl(null);
    if (imageUrlRef.current) URL.revokeObjectURL(imageUrlRef.current);
    imageUrlRef.current = null;
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

  // closeSession explicitly ends the selected Core browser context.
  // closeSession 明确结束所选 Core 浏览器上下文。
  async function closeSession(): Promise<void> {
    if (!selectedSessionId || !lease) {
      return;
    }
    try {
      await requestCoreBrowser(props.ownerSessionId, "browser.session.close", {
        sessionId: selectedSessionId,
        leaseSecret: lease.leaseSecret,
      });
      if (!isCurrentSession(selectedSessionId)) return;
      leaseRef.current = null;
      setLease(null);
      setSelectedSessionId(null);
      setSelectedPageId(null);
      await refreshSessions();
      if (isCurrentOwner()) setError(null);
    } catch (failure) {
      if (isCurrentSession(selectedSessionId)) setError(String(failure));
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-3 overflow-auto p-4" aria-label={chinese ? "Core 浏览器" : "Core browser"}>
      <div className="flex flex-wrap items-center gap-2">
        <Globe2 size={18} />
        <strong>{chinese ? "Core 浏览器" : "Core Browser"}</strong>
        <Button type="button" size="sm" onClick={() => void createSession()} disabled={busy}>
          <Plus size={14} />{chinese ? "新建会话" : "New session"}
        </Button>
        <Button type="button" size="sm" variant="outline" onClick={() => {
          // requestSequence keeps an old manual refresh failure from replacing a newer list.
          // requestSequence 防止旧的手动刷新失败覆盖更新的列表。
          const requestSequence = sessionRequestSequence.current + 1;
          void refreshSessions().catch((failure: unknown) => {
            if (ownerSessionRef.current === props.ownerSessionId
              && requestSequence > appliedSessionSequence.current
              && requestSequence > failedSessionSequence.current) {
              failedSessionSequence.current = requestSequence;
              setPollErrors((current) => ({ ...current, sessions: String(failure) }));
            }
          });
        }}>
          <RefreshCw size={14} />{chinese ? "刷新" : "Refresh"}
        </Button>
      </div>
      <div className="flex flex-wrap gap-2" role="tablist" aria-label={chinese ? "浏览器会话" : "Browser sessions"}>
        {sessions.filter((session) => session.status !== "closed").map((session) => (
          <button key={session.sessionId} type="button" role="tab"
            aria-selected={selectedSessionId === session.sessionId}
            className={`rounded border px-3 py-1 text-xs ${selectedSessionId === session.sessionId ? "bg-primary text-primary-foreground" : "bg-background"}`}
            onClick={() => void selectSession(session.sessionId)}>
            {session.sessionId.slice(0, 8)} · {session.status} · {session.pageCount}
          </button>
        ))}
      </div>
      {selectedSession ? (
        <>
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <span>{chinese ? "控制权" : "Control"}: {selectedSession.leaseState}</span>
            <span>{chinese ? "宿主" : "Host"}: {selectedSession.status}</span>
            <span>{chinese ? "观察流" : "Stream"}: {streamStatus}</span>
            {selectedSession.blockedReason ? <span className="text-destructive">{selectedSession.blockedReason}</span> : null}
            {ownsControl ? (
              <Button type="button" size="sm" variant="outline" onClick={() => void releaseControl()}>{chinese ? "交还控制权" : "Release control"}</Button>
            ) : selectedSession.status === "running" ? (
              <Button type="button" size="sm" variant="outline" onClick={() => void acquireControl()} disabled={selectedSession.leaseState === "blocked"}>
                {selectedSession.leaseState === "free" ? (chinese ? "取得控制权" : "Take control") : (chinese ? "明确接管" : "Take over")}
              </Button>
            ) : null}
            <Button type="button" size="sm" variant="outline" disabled={!ownsControl} onClick={() => void closeSession()}><X size={14} />{chinese ? "关闭会话" : "Close session"}</Button>
          </div>
          <div className="flex flex-wrap items-center gap-2" role="tablist" aria-label={chinese ? "浏览器页面" : "Browser pages"}>
            {pages.map((page) => (
              <button key={page.pageId} type="button" role="tab" aria-selected={page.pageId === selectedPageId}
                className={`max-w-56 truncate rounded border px-2 py-1 text-xs ${page.pageId === selectedPageId ? "bg-accent" : "bg-background"}`}
                title={page.url} onClick={() => {
                  if (page.pageId === selectedPageId) return;
                  for (const key of heldKeys.current) queueInput("key", { type: "up", key });
                  heldKeys.current.clear();
                  // Releasing the old page's pointer before selection prevents a stranded drag.
                  // 切换选择前释放旧页面的指针，避免拖拽被遗留。
                  const held = heldPointer.current;
                  if (held) queueInput("pointer", { type: "up", button: held.button, x: held.x, y: held.y });
                  heldPointer.current = null;
                  setSelectedPageId(page.pageId);
                }}>
                {page.pageId} · {page.url || "about:blank"}
              </button>
            ))}
            <Button type="button" size="sm" variant="outline" disabled={!ownsControl || busy} onClick={() => void createPage()}>
              <Plus size={14} />{chinese ? "新页面" : "New page"}
            </Button>
          </div>
          <form className="flex flex-wrap items-center gap-2" onSubmit={(event) => void navigate(event)}>
            <input ref={addressRef} className="min-w-52 flex-1 rounded border bg-background px-3 py-2 text-sm" value={address}
              onChange={(event) => setAddress(event.target.value)} aria-label={chinese ? "页面地址" : "Page URL"} />
            <Button type="submit" size="sm" disabled={!ownsControl || !selectedPage}>{chinese ? "前往" : "Go"}</Button>
            <Select disabled={!ownsControl}
              value={selectedSession.viewportSize ? `${selectedSession.viewportSize.width}x${selectedSession.viewportSize.height}` : "1280x800"}
              onValueChange={(value) => void resizeViewport(value)}>
              <SelectTrigger className="w-auto min-w-32 text-xs" aria-label={chinese ? "浏览器视口" : "Browser viewport"}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="1280x800">1280×800</SelectItem>
                <SelectItem value="1024x768">1024×768</SelectItem>
                <SelectItem value="390x844">390×844</SelectItem>
              </SelectContent>
            </Select>
          </form>
          <div className="flex min-h-[360px] flex-1 items-center justify-center overflow-hidden rounded border bg-neutral-900">
            {imageUrl && selectedPage ? (
              <div ref={projectionRef} className={`relative h-full w-full outline-none ${ownsControl ? "cursor-crosshair touch-none" : "cursor-default"}`}
                role="img" aria-label={chinese ? "Core 浏览器实时页面" : "Live Core browser page"} tabIndex={ownsControl ? 0 : -1}
                onPointerDown={pressPointer}
                onPointerMove={movePointer}
                onPointerUp={releasePointer}
                onPointerCancel={releasePointer}
                onLostPointerCapture={releasePointer}
                onContextMenu={(event) => { if (ownsControl) event.preventDefault(); }}
                onKeyDown={(event) => {
                  if (!ownsControl || event.repeat) return;
                  event.preventDefault();
                  heldKeys.current.add(event.key);
                  queueInput("key", { type: "down", key: event.key });
                }}
                onKeyUp={(event) => {
                  if (!ownsControl) return;
                  event.preventDefault();
                  heldKeys.current.delete(event.key);
                  queueInput("key", { type: "up", key: event.key });
                }}
                onBlur={() => {
                  for (const key of heldKeys.current) queueInput("key", { type: "up", key });
                  heldKeys.current.clear();
                }}>
                <img className="pointer-events-none h-full w-full object-contain" src={imageUrl} alt="" draggable={false} />
              </div>
            ) : <span className="text-sm text-white/70">{chinese ? "正在等待浏览器画面…" : "Waiting for browser frame…"}</span>}
          </div>
          {dialogs.filter((dialog) => dialog.pageId === selectedPageId).map((dialog) => (
            <div key={dialog.dialogId} role="alertdialog" className="flex flex-wrap items-center gap-2 rounded border p-3 text-sm">
              <strong>{dialog.dialogType}</strong><span>{dialog.message}</span>
              {dialog.dialogType === "prompt" ? <input className="rounded border bg-background px-2 py-1"
                value={dialogText} onChange={(event) => setDialogText(event.target.value)} disabled={!ownsControl} /> : null}
              <Button type="button" size="sm" disabled={!ownsControl} onClick={() => void answerDialog(dialog, true)}>
                {chinese ? "确定" : "Accept"}
              </Button>
              <Button type="button" size="sm" variant="outline" disabled={!ownsControl}
                onClick={() => void answerDialog(dialog, false)}>{chinese ? "取消" : "Dismiss"}</Button>
            </div>
          ))}
          {fileChoosers.filter((chooser) => chooser.pageId === selectedPageId).map((chooser) => (
            <label key={chooser.chooserId} className="flex flex-wrap items-center gap-2 rounded border p-3 text-sm">
              <span>{chinese ? "页面请求选择文件" : "Page requests a file"}</span>
              <input type="file" disabled={!ownsControl} onChange={(event) => void submitFileChooser(chooser, event.target.files?.[0])} />
            </label>
          ))}
          {frame ? <div className="text-xs text-muted-foreground">{selectedPage?.pageId} · {frame.width}×{frame.height} · #{frame.sequence}</div> : null}
          <div className="flex flex-wrap items-center gap-2">
            <input className="min-w-52 flex-1 rounded border bg-background px-3 py-2 text-sm" value={textInput}
              onChange={(event) => setTextInput(event.target.value)} placeholder={chinese ? "输入文字（支持中文输入法）" : "Type text to send"}
              aria-label={chinese ? "发送到页面的文字" : "Text to send to page"} />
            <Button type="button" size="sm" disabled={!ownsControl || !frame || !textInput}
              onClick={() => { queueInput("text", { text: textInput }); setTextInput(""); }}>
              {chinese ? "发送文字" : "Send text"}
            </Button>
          </div>
          {downloads.length > 0 ? (
            <div className="flex flex-wrap items-center gap-2 text-xs" aria-label={chinese ? "浏览器下载" : "Browser downloads"}>
              <span>{chinese ? "下载" : "Downloads"}</span>
              {downloads.map((download) => (
                <Button key={download.downloadId} type="button" size="sm" variant="outline"
                  disabled={download.status !== "completed"} onClick={() => void saveDownload(download)}>
                  <Download size={14} />{download.fileName} · {download.status}
                </Button>
              ))}
            </div>
          ) : null}
        </>
      ) : <p className="text-sm text-muted-foreground">{chinese ? "创建会话后，浏览器在 Core 后台运行；当前窗口只观察和控制它。" : "Create a session to run a Core-owned browser in the background."}</p>}
      {error || Object.values(pollErrors).some(Boolean) ? <div role="alert" className="rounded border border-destructive p-3 text-sm text-destructive">{error ?? Object.values(pollErrors).find(Boolean)}</div> : null}
    </div>
  );
}
