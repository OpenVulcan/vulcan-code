// @vitest-environment jsdom
import { act, createRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ConversationTurnSeatList } from "./conversation-turn-seat";
import {
  CONVERSATION_TURN_WINDOW_EXPAND_EVENT,
  ESTIMATED_TURN_HEIGHT_PX,
  type ConversationTurnWindowRequest,
} from "./conversation-turn-window";
import { ConversationTimelineProjectionStore, type ConversationTurnProjection } from "./conversation-timeline-projection-store";
import { ConversationMeasuredCollapseProvider, useConversationMeasuredExpansion } from "./conversation-measured-collapse";
import type { ConversationTimelineItem } from "./conversation-model";

// host and root own only the isolated mounted React tree.
// host 与 root 仅持有隔离挂载的 React 树。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.useFakeTimers(); vi.setSystemTime(1000);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.useRealTimers(); vi.unstubAllGlobals(); });

/** Creates a typed process projection with an explicit live-clock owner.
 * 创建具有明确实时计时所有者的类型化过程投影。
 * key identifies the row and running selects clock admission; returns one synthetic row.
 * key 标识行，running 选择计时准入；返回一个合成行。
 */
function processRow(key: string, running: boolean): ConversationTurnProjection {
  return { key, items: [{ kind: "process", process: {
    id: key, items: [], guidanceItems: [], collapsible: true, defaultCollapsed: false,
    running, stopped: false, startedAtMs: 1000, durationMs: running ? null : 100,
  } }] };
}

it("never re-renders a seat while time passes, even when its process is running", async () => {
  // history and live have separate keys and immutable row snapshots; the clock belongs to leaf labels only.
  // history 与 live 具有独立的键和不可变行快照；计时只属于叶子标签。
  const history = processRow("history", false);
  const live = processRow("live", true);
  const store = new ConversationTimelineProjectionStore();
  const renderItem = vi.fn((item: ConversationTimelineItem) => <p>{item.kind === "process" ? item.process.id : "unexpected"}</p>);
  const keys = [history.key, live.key];
  const rows = new Map([[history.key, history], [live.key, live]]);
  await act(async () => root.render(<ConversationTurnSeatList cacheScopeKey="one" rowKeys={keys} rows={rows} store={store} renderItem={renderItem} />));
  // The parent commits rows after children mount, as the production layout effect does.
  // 父层在子项挂载后提交行，与生产布局效果一致。
  await act(async () => { store.reconcile([history, live]); store.flushNotifications(); });
  renderItem.mockClear();
  await act(async () => vi.advanceTimersByTime(5000));
  expect(renderItem).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
});

it("evicts offscreen bodies, retains measured shells, and remounts the current projection", async () => {
  // Drive the browser observer explicitly because jsdom has no viewport geometry.
  // jsdom 无视口几何，显式驱动浏览器观察器。
  let report: IntersectionObserverCallback | undefined;
  const observer = { observe: vi.fn(), unobserve: vi.fn(), disconnect: vi.fn() };
  vi.stubGlobal("IntersectionObserver", vi.fn(function MockObserver(callback: IntersectionObserverCallback) {
    report = callback;
    return observer;
  }));
  // Keep the DOM owners stable across all observer deliveries.
  // 在各次观察器派发间保持 DOM 所有者稳定。
  const scrollRef = createRef<HTMLElement>();
  // Create two rows and subscribe the mounted body to the production projection store.
  // 创建两条回合，并让已挂载正文订阅生产投影仓库。
  const history = processRow("history", false);
  const live = processRow("live", false);
  const store = new ConversationTimelineProjectionStore();
  const rows = new Map([[history.key, history], [live.key, live]]);
  // Render a body marker whose text distinguishes initial and current snapshots.
  // 渲染正文标记，用文本区分初始投影和当前投影。
  const renderItem = (item: ConversationTimelineItem) => (
    <p>{item.kind === "process" ? `${item.process.id}:${item.process.running}` : ""}</p>
  );
  await act(async () => root.render(
    <section ref={scrollRef}><div className="vk-transcript-inner"><div className="vk-timeline">
      <ConversationTurnSeatList
        cacheScopeKey="one" rowKeys={[history.key, live.key]} rows={rows}
        store={store} renderItem={renderItem} scrollRef={scrollRef}
      />
    </div></div></section>,
  ));
  // Resolve shells by their stable keys, including those without a body.
  // 按稳定键查找外壳，包含尚无正文的外壳。
  const oldShell = host.querySelector<HTMLElement>('[data-conversation-turn-shell="history"]')!;
  const newShell = host.querySelector<HTMLElement>('[data-conversation-turn-shell="live"]')!;
  vi.spyOn(oldShell, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 0, 200, 731));
  expect(oldShell.style.height).toBe(`${ESTIMATED_TURN_HEIGHT_PX}px`);
  expect(host.querySelectorAll("p")).toHaveLength(0);
  expect(observer.observe).toHaveBeenCalledWith(oldShell);
  // Each synthetic intersection is limited to the fields consumed by the observer.
  // 每个合成相交事件只提供观察器使用的字段。
  const deliver = (changes: { target: Element; isIntersecting: boolean }[]) => {
    report?.(changes as IntersectionObserverEntry[], observer as unknown as IntersectionObserver);
  };
  await act(async () => deliver([{ target: oldShell, isIntersecting: true }]));
  expect(host.textContent).toBe("history:false");
  await act(async () => deliver([
    { target: oldShell, isIntersecting: false },
    { target: newShell, isIntersecting: true },
  ]));
  expect(host.textContent).toBe("live:false");
  expect(oldShell.style.height).toBe("731px");
  expect(oldShell.isConnected).toBe(true);

  // Update the offscreen row and verify remount reads the latest committed state.
  // 更新屏外回合，验证重新挂载读取最近提交的状态。
  await act(async () => {
    store.reconcile([processRow("history", true), live]);
    store.flushNotifications();
  });
  // The existing DOM-based find path temporarily mounts the whole loaded transcript.
  // 既有基于 DOM 的查找路径临时挂载全部已加载转录。
  const find = (active: boolean) => scrollRef.current?.dispatchEvent(new CustomEvent<ConversationTurnWindowRequest>(
    CONVERSATION_TURN_WINDOW_EXPAND_EVENT,
    { detail: { reason: "find", active } },
  ));
  await act(async () => { find(true); });
  expect(host.textContent).toBe("history:truelive:false");
  await act(async () => { find(false); });
  expect(host.textContent).toBe("live:false");
  expect(oldShell.style.height).toBe("731px");
});

it("applies row notifications through a memoized list and clears clocks on scope removal", async () => {
  // store receives content-only updates while the parent retains the same key array.
  // store 接收纯内容更新，同时父层保持同一键数组。
  const store = new ConversationTimelineProjectionStore();
  const first = processRow("one", false);
  const rows = new Map([[first.key, first]]);
  const keys = [first.key];
  const renderItem = vi.fn((item: ConversationTimelineItem) => <p>{item.kind === "process" && item.process.running ? "running" : "idle"}</p>);
  await act(async () => root.render(<ConversationTurnSeatList cacheScopeKey="a" rowKeys={keys} rows={rows} store={store} renderItem={renderItem} />));
  await act(async () => { store.reconcile([first]); store.flushNotifications(); });
  await act(async () => { store.reconcile([processRow("one", true)]); store.flushNotifications(); });
  expect(host.textContent).toBe("running");
  await act(async () => root.render(<ConversationTurnSeatList cacheScopeKey="b" rowKeys={[]} rows={new Map()} store={new ConversationTimelineProjectionStore()} renderItem={renderItem} />));
  expect(host.textContent).toBe("");
});

/** Reads and changes one exact fold identity in the production memory provider.
 * 读取并修改生产记忆提供者中的一个精确折叠身份。
 * id identifies the fold and rendered observes leaf renders; returns one local button.
 * id 标识折叠，rendered 观察叶子渲染；返回一个本地按钮。
 */
function Fold({ id, rendered }: { id: string; rendered: (id: string) => void }) {
  const [expanded, setExpanded] = useConversationMeasuredExpansion(id, false);
  rendered(id);
  return <button onClick={() => setExpanded(!expanded)}>{id}:{String(expanded)}</button>;
}

it("isolates fold updates and does not persist an old scope into a new conversation", async () => {
  // changed records persistence only; no user data or service action is involved.
  // changed 仅记录持久化回调；不涉及用户数据或服务操作。
  const changed = vi.fn();
  const rendered = vi.fn();
  const children = <><Fold id="one" rendered={rendered} /><Fold id="two" rendered={rendered} /></>;
  await act(async () => root.render(<ConversationMeasuredCollapseProvider scopeId="a" initialMemory={{}} onMemoryChange={changed}>{children}</ConversationMeasuredCollapseProvider>));
  rendered.mockClear();
  await act(async () => host.querySelector("button")!.click());
  expect(host.textContent).toBe("one:truetwo:false");
  expect(rendered.mock.calls.every(([id]) => id === "one")).toBe(true);
  expect(changed).toHaveBeenLastCalledWith({ one: { expanded: true } });
  changed.mockClear();
  await act(async () => root.render(<ConversationMeasuredCollapseProvider scopeId="b" initialMemory={{ two: { expanded: true } }} onMemoryChange={changed}>{children}</ConversationMeasuredCollapseProvider>));
  expect(host.textContent).toBe("one:falsetwo:true");
  expect(changed).not.toHaveBeenCalled();
});
