import * as React from "react";
import { ExternalLink, Plus, RefreshCw, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useI18n } from "@/i18n";
import {
  readElectronLocalBrowserBridge,
  type ElectronLocalBrowserSnapshot,
  type ElectronLocalBrowserViewport,
} from "@/shell/electronBridge";

// localBrowserAddress validates an explicit HTTP destination without granting a page workbench access.
// localBrowserAddress 校验明确的 HTTP 目标，不向页面授予工作台访问权限。
// input is the user's address; returns a canonical URL or throws a visible validation error.
// input 是用户地址；返回规范 URL，或抛出可见的校验错误。
function localBrowserAddress(input: string): string {
  const value = input.trim();
  if (!value) throw new Error("Enter a website address.");
  const url = new URL(/^[a-z][a-z\d+.-]*:/i.test(value) ? value : `https://${value}`);
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("Only HTTP and HTTPS websites can be opened here.");
  }
  return url.href;
}

// ClientBrowserArea gives this client a temporary browser unrelated to Core-owned AI pages.
// ClientBrowserArea 为本客户端提供与 Core 自有 AI 页面无关的临时浏览器。
// Returns an embedded native view in Electron or a deliberate new-tab link in ordinary WebUI.
// 在 Electron 中返回内嵌原生视图，在普通 WebUI 中返回明确的新标签链接。
export function ClientBrowserArea() {
  // locale controls only visible labels, not navigation or trust boundaries.
  // locale 仅控制可见文案，不改变导航或信任边界。
  const { locale } = useI18n();
  // chinese selects concise Chinese labels for the local client.
  // chinese 为本地客户端选择简洁的中文文案。
  const chinese = locale === "zh-CN";
  // bridge exists only in an Electron workbench with the isolated native view capability.
  // bridge 仅在具备隔离原生视图能力的 Electron 工作台存在。
  const bridge = React.useMemo(readElectronLocalBrowserBridge, []);
  // snapshot is the current GUI-local tab list, never a Core session list.
  // snapshot 是当前 GUI 本地标签列表，绝不是 Core 会话列表。
  const [snapshot, setSnapshot] = React.useState<ElectronLocalBrowserSnapshot>({ activeId: null, tabs: [] });
  // address is edited by this client before an explicit navigation action.
  // address 由本客户端编辑，并仅在明确导航操作后使用。
  const [address, setAddress] = React.useState("");
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
    const unsubscribe = bridge.subscribeChanged((next) => {
      changeRevision += 1;
      if (!cancelled) setSnapshot(next);
    });
    const requestedRevision = changeRevision;
    void bridge.snapshot().then((initial) => {
      if (!cancelled && changeRevision === requestedRevision) setSnapshot(initial);
    }).catch((reason: unknown) => {
      if (!cancelled) setError(String(reason));
    });
    return () => {
      cancelled = true;
      unsubscribe();
      void bridge.viewport(null);
    };
  }, [bridge]);

  // activeTab is this GUI's selected temporary page, not the selected AI automation target.
  // activeTab 是当前 GUI 选中的临时页面，不是 AI 自动化目标。
  const activeTab = snapshot.tabs.find((tab) => tab.id === snapshot.activeId) ?? null;
  React.useEffect(() => {
    if (activeTab && document.activeElement !== addressInput.current) setAddress(activeTab.url);
  }, [activeTab?.id, activeTab?.url]);

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
      const modalOpen = document.querySelector('[role="dialog"][data-state="open"], [aria-modal="true"]') !== null;
      const visible = bounds.width > 0 && bounds.height > 0 && bounds.bottom > 0 && bounds.right > 0
        && bounds.top < window.innerHeight && bounds.left < window.innerWidth
        && getComputedStyle(element).visibility !== "hidden" && !modalOpen;
      const viewport: ElectronLocalBrowserViewport = {
        x: bounds.left, y: bounds.top, width: bounds.width, height: bounds.height,
        viewportWidth: window.innerWidth, viewportHeight: window.innerHeight, visible,
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
  }, [bridge, snapshot.activeId]);

  // submit opens or navigates the selected GUI-local page after address validation.
  // submit 在地址校验后打开或导航选中的 GUI 本地页面。
  // event is the address form submission; returns after the native request is queued.
  // event 是地址表单提交；在原生请求入队后返回。
  async function submit(event: React.FormEvent): Promise<void> {
    event.preventDefault();
    try {
      const url = localBrowserAddress(address);
      setError(null);
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

  return <div className="flex h-full min-h-0 flex-col gap-2 p-2">
    {bridge && <div className="flex min-h-8 gap-1 overflow-x-auto" aria-label={chinese ? "本地浏览标签" : "Local browser tabs"}>
      {snapshot.tabs.map((tab) => <div key={tab.id} className="flex max-w-44 shrink-0 items-center rounded border">
        <Button type="button" variant={tab.id === snapshot.activeId ? "secondary" : "ghost"} size="sm"
          className="min-w-0 flex-1 truncate" title={tab.title || tab.url}
          onClick={() => void bridge.select(tab.id).catch((reason: unknown) => setError(String(reason)))}>
          {tab.loading ? "… " : ""}{tab.title || tab.url}
        </Button>
        <Button type="button" variant="ghost" size="icon" className="h-7 w-7 shrink-0"
          aria-label={chinese ? "关闭本地标签" : "Close local tab"}
          onClick={() => void bridge.close(tab.id).catch((reason: unknown) => setError(String(reason)))}><X className="h-3 w-3" /></Button>
      </div>)}
      <Button type="button" variant="outline" size="icon" className="h-8 w-8 shrink-0"
        aria-label={chinese ? "新建本地标签" : "New local tab"}
        onClick={() => void bridge.open("about:blank").catch((reason: unknown) => setError(String(reason)))}><Plus className="h-4 w-4" /></Button>
    </div>}
    <form onSubmit={(event) => void submit(event)} className="flex items-center gap-1">
      <input ref={addressInput} value={address} onChange={(event) => setAddress(event.target.value)}
        aria-label={chinese ? "本地浏览地址" : "Local browser address"}
        placeholder="https://example.com" className="h-8 min-w-0 flex-1 rounded border bg-background px-2 text-sm" />
      <Button type="submit" variant="outline" size="sm" aria-label={chinese ? "打开地址" : "Open address"}>
        {bridge ? <RefreshCw className="h-4 w-4" /> : <ExternalLink className="h-4 w-4" />}
      </Button>
    </form>
    {error && <p role="alert" className="text-xs text-destructive">{error}</p>}
    {bridge ? <div ref={placeholder} className="min-h-0 flex-1 rounded border bg-muted/20">
      {!activeTab && <p className="p-3 text-sm text-muted-foreground">
        {chinese ? "输入地址或新建本地标签。此浏览器仅属于当前客户端。" : "Enter an address or create a local tab. This browser belongs only to this client."}
      </p>}
    </div> : <div className="flex min-h-0 flex-1 flex-col gap-2 rounded border p-3 text-sm text-muted-foreground">
      <p>{chinese ? "网页客户端将在新标签打开临时页面。AI 浏览器仍由 Core 单独管理。" : "This client opens temporary pages in a new tab. Core manages the AI browser separately."}</p>
      {(() => {
        try {
          const url = localBrowserAddress(address);
          return <a href={url} target="_blank" rel="noopener noreferrer" className="break-all text-primary underline">
            {chinese ? "直接打开链接" : "Open link directly"}: {url}
          </a>;
        } catch {
          return null;
        }
      })()}
    </div>}
  </div>;
}
