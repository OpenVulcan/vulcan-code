// Authoritative Electron preload bridge contract shared by the runtime probe and the shell facade.
// 由运行时探测与壳门面共享的权威 Electron 预加载桥契约。
//
// Both consumers must read the same object shape. Keeping one definition here prevents the drift
// where the runtime probe and the shell facade expected different method names on the same global.
// 两个使用方必须读取同一对象形状。在此保留唯一定义，避免运行时探测与壳门面在同一个全局对象上
// 期待不同方法名而漂移。

import type {
  ShellNotificationActivation,
  ShellNotificationPermission,
  ShellNotificationRequest,
} from "@/host-shell/protocol";
import type { ServiceRestartReason } from "@/service/app-update-events";

// ElectronDesktopBridge is the exact native window and notification surface the preload exposes.
// ElectronDesktopBridge 是预加载暴露的精确原生窗口与通知表面。
export interface ElectronDesktopBridge {
  // Reports whether this exact executable has an isolated native notification identity.
  // 表示当前可执行文件是否拥有隔离的原生通知身份。
  nativeNotificationsAvailable: boolean;
  // Opens developer tools for the current workbench window when the host provides this capability.
  // 宿主提供此能力时打开当前工作台窗口的开发者工具。
  openDeveloperTools?: () => Promise<void>;
  // Reads plain text from the desktop clipboard when the current host supports it.
  // 当前宿主支持时读取桌面剪贴板纯文本。
  readClipboardText?: () => Promise<string>;
  // Writes plain text to the desktop clipboard when the current host supports it.
  // 当前宿主支持时写入桌面剪贴板纯文本。
  writeClipboardText?: (value: string) => Promise<void>;
  // Dispatches native Paste to the focused workbench input when the host supports it.
  // 宿主支持时向聚焦的工作台输入框派发原生粘贴。
  pasteClipboard?: () => Promise<void>;
  // Minimizes the current desktop window.
  // 最小化当前桌面窗口。
  minimizeWindow: () => Promise<void>;
  // Toggles the current desktop window maximize state.
  // 切换当前桌面窗口最大化状态。
  toggleWindowMaximize: () => Promise<void>;
  // Returns whether the current desktop window is maximized.
  // 返回当前桌面窗口是否处于最大化状态。
  isWindowMaximized: () => Promise<boolean>;
  // Enters or leaves native full screen, the macOS green-button action.
  // 进入或离开原生全屏，即 macOS 绿色按钮的操作。
  toggleWindowFullScreen: () => Promise<void>;
  // Returns whether the current desktop window is in native full screen.
  // 返回当前桌面窗口是否处于原生全屏。
  isWindowFullScreen: () => Promise<boolean>;
  // Places the macOS window buttons in the reported title strip and resolves with their right edge; rejects elsewhere.
  // 将 macOS 窗口按钮放入上报的标题条并返回其右边缘；其他平台上拒绝。
  placeNativeButtons: (placement: { left: number; height: number }) => Promise<number>;
  // Closes the current desktop window without stopping Core.
  // 关闭当前桌面窗口，不停止 Core。
  closeWindow: () => Promise<void>;
  // Subscribes to native window resize events and returns the unsubscribe function.
  // 订阅原生窗口尺寸变化事件并返回取消订阅函数。
  subscribeWindowResize: (handler: () => void) => Promise<() => void>;
  // Returns whether the current desktop window is minimized.
  // 返回当前桌面窗口是否已最小化。
  isWindowMinimized: () => Promise<boolean>;
  // Returns whether the current desktop window has input focus.
  // 返回当前桌面窗口是否拥有输入焦点。
  isWindowFocused: () => Promise<boolean>;
  // Returns whether the current desktop window is visible.
  // 返回当前桌面窗口是否可见。
  isWindowVisible: () => Promise<boolean>;
  // Subscribes to native window focus changes and returns the unsubscribe function.
  // 订阅原生窗口焦点变化并返回取消订阅函数。
  subscribeWindowFocusChange: (handler: () => void) => Promise<() => void>;
  // Returns whether the host can present local notifications without prompting.
  // 返回宿主能否在不提示用户的情况下展示本地通知。
  getNotificationPermission: () => Promise<ShellNotificationPermission>;
  // Requests notification permission for enabled startup or an explicit user action.
  // 在已启用的启动流程或明确用户操作时请求通知权限。
  requestNotificationPermission: () => Promise<ShellNotificationPermission>;
  // Presents one localized local notification through the native host.
  // 通过原生宿主展示一条已本地化的本地通知。
  showNotification: (request: ShellNotificationRequest) => Promise<void>;
  // Subscribes to native notification activation events.
  // 订阅原生通知激活事件。
  subscribeNotificationActivation: (
    handler: (activation: ShellNotificationActivation) => void,
  ) => Promise<() => void>;
}

// ElectronLocalBrowserTab is a GUI-local page and never a Core browser session.
// ElectronLocalBrowserTab 是 GUI 本地页面，绝不是 Core 浏览器会话。
export interface ElectronLocalBrowserTab {
  // id is valid only for this desktop GUI lifetime.
  // id 仅在当前桌面 GUI 生命周期内有效。
  id: string;
  // title is untrusted page text.
  // title 是不受信任的页面文本。
  title: string;
  // url is the current top-level page address.
  // url 是当前顶层页面地址。
  url: string;
  // faviconUrl is supplied by Electron's native favicon event; null selects the default icon.
  // faviconUrl 来自 Electron 原生图标事件；为空时显示默认图标。
  faviconUrl: string | null;
  // loading reports native navigation activity.
  // loading 报告原生导航活动。
  loading: boolean;
  // Native navigation history determines control availability.
  // 原生导航历史决定控件是否可用。
  canGoBack: boolean;
  canGoForward: boolean;
}

// ElectronLocalBrowserSnapshot is the native temporary-tab projection.
// ElectronLocalBrowserSnapshot 是原生临时标签投影。
export interface ElectronLocalBrowserSnapshot {
  // ownerSessionId names the conversation whose tabs this GUI window is projecting.
  // ownerSessionId 指明此 GUI 窗口正在投影哪个会话的标签。
  ownerSessionId: string | null;
  // activeId selects the page shown in the sidebar.
  // activeId 选择侧栏中显示的页面。
  activeId: string | null;
  // tabs contains only this client's temporary pages.
  // tabs 仅包含本客户端的临时页面。
  tabs: ElectronLocalBrowserTab[];
}

// ElectronLocalBrowserElementPick is one page element the user pointed at, measured in the page's CSS pixels.
// ElectronLocalBrowserElementPick 是用户指向的一个页面元素，以页面 CSS 像素度量。
export interface ElectronLocalBrowserElementPick {
  // url and title identify the page at the moment of the pick.
  // url 与 title 标识选中时的页面。
  url: string;
  title: string;
  // selector matched exactly this element when it was picked.
  // selector 在选中时恰好匹配此元素。
  selector: string;
  // tag is the lower-case element name; text is a short excerpt of its visible text.
  // tag 是小写元素名；text 是其可见文本的简短摘录。
  tag: string;
  text: string;
  // rect is the element's border box relative to a page viewport of `viewport` size.
  // rect 是元素边框盒相对于尺寸为 `viewport` 的页面视口的位置。
  rect: { x: number; y: number; width: number; height: number };
  viewport: { width: number; height: number };
  // screenshot is a PNG of the visible page with the element outlined.
  // screenshot 是可见页面的 PNG，已标出该元素。
  screenshot: Uint8Array;
}

// ElectronLocalBrowserViewport maps one DOM placeholder to the native view bounds.
// ElectronLocalBrowserViewport 将一个 DOM 占位区域映射到原生视图边界。
export interface ElectronLocalBrowserViewport {
  // x and y locate the visible placeholder in CSS pixels.
  // x 与 y 以 CSS 像素定位可见占位区域。
  x: number;
  y: number;
  // width and height describe its visible extent.
  // width 与 height 描述其可见范围。
  width: number;
  height: number;
  // viewportWidth and viewportHeight map browser zoom onto native content coordinates.
  // viewportWidth 与 viewportHeight 将浏览器缩放映射到原生内容坐标。
  viewportWidth: number;
  viewportHeight: number;
  // visible is false while a workbench modal is open or the panel is hidden.
  // visible 在工作台模态框打开或面板隐藏时为 false。
  visible: boolean;
  // An omitted or null size fills the panel; an explicit CSS size is fitted inside it.
  // 未提供或为空的尺寸填充面板；明确的 CSS 尺寸在面板内适配显示。
  deviceSize?: { width: number; height: number } | null;
}

// ElectronLocalBrowserBridge exposes tab operations without WebContents or Core credentials.
// ElectronLocalBrowserBridge 公开标签操作，不公开 WebContents 或 Core 凭据。
export interface ElectronLocalBrowserBridge {
  // initialize clears old native pages before the workbench mounts; resolves after cleanup.
  // initialize 在工作台挂载前清除旧原生页面；清理完成后返回。
  initialize: () => Promise<void>;
  // setOwner shows only tabs associated with one conversation in this GUI window.
  // setOwner 仅显示此 GUI 窗口中属于一个会话的标签。
  setOwner: (ownerSessionId: string | null) => Promise<ElectronLocalBrowserSnapshot>;
  // snapshot returns the current client-local tabs.
  // snapshot 返回当前客户端本地标签。
  snapshot: () => Promise<ElectronLocalBrowserSnapshot>;
  // open creates one sandboxed website or file tab and returns its id.
  // open 创建一个沙箱网页或文件标签并返回其标识。
  open: (url: string) => Promise<string>;
  // navigate changes one existing local tab's address.
  // navigate 更改一个已有本地标签的地址。
  navigate: (id: string, url: string) => Promise<void>;
  // Executes one native navigation action for id; resolves after dispatch.
  // 为 id 执行一次原生导航操作；派发后完成。
  history: (id: string, action: 'back' | 'forward' | 'reload' | 'stop') => Promise<void>;
  // Moves id to targetId's position without recreating either page; resolves after moving.
  // 将 id 移动到 targetId 的位置，不重建页面；移动后完成。
  move: (id: string, targetId: string) => Promise<void>;
  // Opens id's inspector in a separate native window; resolves after opening.
  // 在独立原生窗口打开 id 的检查器；打开后完成。
  openDeveloperTools: (id: string) => Promise<void>;
  // Lets the user point at one element of id's page; resolves with it, or null when cancelled. Absent on
  // preloads without element picking.
  // 让用户指向 id 页面中的一个元素；返回该元素，取消时返回 null。不支持元素选择的预加载没有此方法。
  pickElement?: (id: string) => Promise<ElectronLocalBrowserElementPick | null>;
  // Ends a running element pick on id's page; resolves after the page is told.
  // 结束 id 页面上进行中的元素选择；通知页面后完成。
  cancelElementPick?: (id: string) => Promise<void>;
  // Opens id's actual website or file URL externally; resolves after the OS request.
  // 在外部打开 id 的实际网页或文件地址；操作系统请求后完成。
  openExternal: (id: string) => Promise<void>;
  // Opens a validated conversation website or file URL without creating a tab; resolves after the OS request.
  // 打开经校验的会话网页或文件 URL，不创建标签；操作系统请求后完成。
  openExternalUrl: (url: string) => Promise<void>;
  // select displays one existing local tab.
  // select 显示一个已有本地标签。
  select: (id: string) => Promise<void>;
  // close destroys one local tab.
  // close 销毁一个本地标签。
  close: (id: string) => Promise<void>;
  // Applies the in-app decision to ownerSessionId's pages; resolves true only after destruction.
  // 将应用内决策应用于 ownerSessionId 的页面；销毁完成后才返回 true。
  closeOwner: (ownerSessionId: string, confirmed: boolean) => Promise<boolean>;
  // viewport updates or hides the native sidebar view.
  // viewport 更新或隐藏侧栏中的原生视图。
  viewport: (rect: ElectronLocalBrowserViewport | null) => Promise<void>;
  // subscribeChanged observes local tab changes and returns synchronous cleanup.
  // subscribeChanged 观察本地标签变化并返回同步清理函数。
  subscribeChanged: (handler: (snapshot: ElectronLocalBrowserSnapshot) => void) => () => void;
}

// ElectronLocalFileInspection reports a path's state before the workbench offers OS actions for it.
// ElectronLocalFileInspection 在工作台为路径提供操作系统操作前报告其状态。
export interface ElectronLocalFileInspection {
  // Whether a file or folder exists at the path.
  // 路径处是否存在文件或文件夹。
  exists: boolean;
  // Whether the default program may open it; false for programs, scripts and folders.
  // 能否交给默认程序打开；程序、脚本与文件夹为 false。
  opensWithDefaultApp: boolean;
}

// ElectronLocalFileBridge hands one absolute path to the OS; the main process rechecks every request.
// ElectronLocalFileBridge 将单个绝对路径交给操作系统；主进程复核每个请求。
export interface ElectronLocalFileBridge {
  // Returns the native path for a user-provided File, or an empty string for an in-memory File.
  // 返回用户提供的 File 的原生路径，内存 File 返回空字符串。
  pathForFile?: (file: File) => string;
  // inspect resolves with the path's state.
  // inspect 返回路径状态。
  inspect: (path: string) => Promise<ElectronLocalFileInspection>;
  // openWithDefaultApp opens a non-runnable file with its default program; rejects for runnable files.
  // openWithDefaultApp 用默认程序打开非可运行文件；可运行文件被拒绝。
  openWithDefaultApp: (path: string) => Promise<void>;
  // showInFolder selects the file in the system file manager.
  // showInFolder 在系统文件管理器中选中该文件。
  showInFolder: (path: string) => Promise<void>;
}

// ElectronDesktopApi narrows the trusted preload surface mounted on window.
// ElectronDesktopApi 收窄挂载在 window 上的受信预加载界面。
interface ElectronDesktopApi {
  // desktop exposes window and notification actions through the isolated preload bridge.
  // desktop 通过隔离预加载桥暴露窗口与通知能力。
  desktop: ElectronDesktopBridge;
  // Native clipboard paths belong to the owning workbench and may be absent on older shells.
  // 原生剪贴板路径归所属工作台，较旧桌面壳可能没有此方法。
  readClipboardFileReferences?: () => Promise<Array<{ path: string; name: string; directory: boolean }>>;
  // service resolves the exact ready Core selected by the GUI owner.
  // service 解析 GUI 所有者选中的精确就绪 Core。
  // restartForUpdate exists on preloads that can relaunch the client after an update restart.
  // restartForUpdate 存在于能在更新重启后重新启动客户端的预加载中。
  service: {
    getConnection: () => Promise<ElectronServiceConnection>;
    restartForUpdate?: (reason: ServiceRestartReason) => Promise<void>;
  };
  // localBrowser is the optional isolated temporary-page surface of this GUI.
  // localBrowser 是此 GUI 可选的隔离临时页面界面。
  localBrowser?: ElectronLocalBrowserBridge;
  // localFile opens conversation file references with the OS; absent on older preloads.
  // localFile 通过操作系统打开会话文件引用；旧预加载没有此项。
  localFile?: ElectronLocalFileBridge;
}

// ElectronServiceConnection carries one authenticated instance in memory, never in navigation URLs.
// ElectronServiceConnection 在内存中携带一个鉴权实例，绝不进入导航 URL。
export interface ElectronServiceConnection {
  // baseUrl names the selected Core HTTP origin.
  // baseUrl 标识选定 Core 的 HTTP 源。
  baseUrl: string;
  // pid identifies the owning process.
  // pid 标识所属进程。
  pid: number;
  // startedAt identifies this process lifetime.
  // startedAt 标识本次进程生命周期。
  startedAt: string;
  // serviceToken authorizes requests to this instance only.
  // serviceToken 仅授权访问本实例。
  serviceToken: string;
  // status is ready only after business bootstrap succeeds.
  // status 仅在业务初始化成功后为 ready。
  status: string;
}

// readElectronServiceConnection returns the selected ready instance, or null outside Electron.
// readElectronServiceConnection 返回选中的就绪实例；非 Electron 环境返回 null。
export async function readElectronServiceConnection(): Promise<ElectronServiceConnection | null> {
  if (typeof window === "undefined") return null;
  const api = (window as unknown as { vulcanDesktop?: ElectronDesktopApi }).vulcanDesktop;
  if (!api) return null;
  if (!api.service || typeof api.service.getConnection !== "function") {
    throw new Error("The desktop service connection bridge is unavailable.");
  }
  return api.service.getConnection();
}

// restartElectronClientForUpdate hands an announced Core restart to the desktop client, which ends
// itself once the old Core exited; returns false outside Electron or on an older preload.
// restartElectronClientForUpdate 将已宣布的 Core 重启交给桌面客户端，客户端在旧 Core 退出后结束自身；
// 在 Electron 之外或旧预加载上返回 false。
export async function restartElectronClientForUpdate(reason: ServiceRestartReason): Promise<boolean> {
  if (typeof window === "undefined") return false;
  const api = (window as unknown as { vulcanDesktop?: ElectronDesktopApi }).vulcanDesktop;
  const restart = api?.service?.restartForUpdate;
  if (typeof restart !== "function") return false;
  await restart(reason);
  return true;
}

// DesktopStartupAppearance is what the desktop startup page needs to look like the WebUI before Core answers.
// DesktopStartupAppearance 是桌面启动页在 Core 应答前与 WebUI 外观一致所需的信息。
export interface DesktopStartupAppearance {
  // Active WebUI language code.
  // 当前 WebUI 语言代码。
  locale: string;
  // Chosen theme; a skin is reported as the light or dark theme it renders.
  // 所选主题；皮肤按其呈现的明暗主题上报。
  theme: string;
  // UI text tier.
  // 界面字号档位。
  text: string;
  // Motion preference.
  // 动效偏好。
  motion: string;
  // Resolved ground color as `#rrggbb`, used as the native window background.
  // 解析后的底色 `#rrggbb`，用作原生窗口背景。
  ground: string;
}

// ElectronStartupBridge connects the WebUI to the desktop startup page shown in the same window.
// ElectronStartupBridge 连接 WebUI 与同一窗口中显示的桌面启动页。
export interface ElectronStartupBridge {
  // True when this window showed the desktop startup page before the WebUI, so the logo intro already played.
  // 当本窗口在 WebUI 之前显示过桌面启动页时为 true，即标志入场动效已经播放。
  surfaceShown: boolean;
  // Remembers the appearance the next desktop startup page uses.
  // 记住下次桌面启动页使用的外观。
  reportAppearance: (appearance: DesktopStartupAppearance) => Promise<void>;
}

// readElectronStartupBridge returns the desktop startup bridge, or null outside Electron or on an incomplete preload.
// readElectronStartupBridge 返回桌面启动桥；在 Electron 之外或预加载不完整时返回 null。
export function readElectronStartupBridge(): ElectronStartupBridge | null {
  if (typeof window === "undefined") return null;
  const startup = (window as unknown as { vulcanDesktop?: { startup?: Partial<ElectronStartupBridge> } }).vulcanDesktop?.startup;
  return startup && typeof startup.surfaceShown === "boolean" && typeof startup.reportAppearance === "function"
    ? (startup as ElectronStartupBridge)
    : null;
}

// electronDesktopBridgeMethodNames lists every method the contract requires at runtime.
// electronDesktopBridgeMethodNames 列出契约在运行时要求的全部方法。
const electronDesktopBridgeMethodNames = [
  "minimizeWindow",
  "toggleWindowMaximize",
  "isWindowMaximized",
  "toggleWindowFullScreen",
  "isWindowFullScreen",
  "placeNativeButtons",
  "closeWindow",
  "subscribeWindowResize",
  "isWindowMinimized",
  "isWindowFocused",
  "isWindowVisible",
  "subscribeWindowFocusChange",
  "getNotificationPermission",
  "requestNotificationPermission",
  "showNotification",
  "subscribeNotificationActivation",
] as const;

// readElectronDesktopBridge returns the trusted preload bridge only when it is complete.
// readElectronDesktopBridge 仅在受信预加载桥完整时返回它。
//
// # Returns / 返回值
// The bridge when every contract method exists, otherwise null so the caller never calls a
// missing method at runtime.
// 当契约方法全部存在时返回该桥，否则返回 null，使调用方不会在运行时调用缺失的方法。
export function readElectronDesktopBridge(): ElectronDesktopBridge | null {
  if (typeof window === "undefined") {
    return null;
  }
  const api = (window as unknown as { vulcanDesktop?: ElectronDesktopApi }).vulcanDesktop;
  if (!api || typeof api.desktop !== "object" || api.desktop === null) {
    return null;
  }
  const desktop = api.desktop as unknown as Record<string, unknown>;
  const complete = electronDesktopBridgeMethodNames.every(
    (name) => typeof desktop[name] === "function",
  ) && typeof desktop.nativeNotificationsAvailable === "boolean";
  return complete ? (api.desktop as ElectronDesktopBridge) : null;
}

// Returns the optional native file clipboard reader independently of window-management APIs.
// 独立于窗口管理 API，返回可选的原生文件剪贴板读取方法。
export function readElectronClipboardFileReferences():
  (() => Promise<Array<{ path: string; name: string; directory: boolean }>>) | undefined {
  if (typeof window === "undefined") return undefined;
  const api = (window as unknown as { vulcanDesktop?: ElectronDesktopApi }).vulcanDesktop;
  return typeof api?.readClipboardFileReferences === "function" ? api.readClipboardFileReferences : undefined;
}

// readElectronLocalBrowserBridge returns the complete temporary-browser bridge when Electron provides it.
// readElectronLocalBrowserBridge 在 Electron 提供完整临时浏览器桥时返回该桥。
// Returns null for ordinary WebUI clients or an incomplete preload.
// 普通 WebUI 客户端或不完整预加载返回 null。
export function readElectronLocalBrowserBridge(): ElectronLocalBrowserBridge | null {
  if (typeof window === "undefined") return null;
  const api = (window as unknown as { vulcanDesktop?: ElectronDesktopApi }).vulcanDesktop;
  const bridge = api?.localBrowser;
  if (!bridge) return null;
  const methods = ["initialize", "setOwner", "snapshot", "open", "navigate", "history", "move", "openDeveloperTools", "openExternal", "openExternalUrl", "select", "close", "closeOwner", "viewport", "subscribeChanged"];
  return methods.every((method) => typeof (bridge as unknown as Record<string, unknown>)[method] === "function")
    ? bridge : null;
}

// readElectronLocalFileBridge returns the local file bridge, or null for web clients and older preloads; the preload
// exposes its methods together, so one of them stands for all.
// readElectronLocalFileBridge 返回本地文件桥；网页客户端与旧预加载返回 null。预加载同时暴露全部方法，因此检查其一即可。
export function readElectronLocalFileBridge(): ElectronLocalFileBridge | null {
  const bridge = typeof window === "undefined" ? undefined : (window as unknown as { vulcanDesktop?: ElectronDesktopApi }).vulcanDesktop?.localFile;
  return typeof bridge?.inspect === "function" ? bridge : null;
}
