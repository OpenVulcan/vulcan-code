// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ConversationTurnSeatList } from "./conversation-turn-seat";
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
