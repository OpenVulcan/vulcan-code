// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { HOVER_CARD_OPEN_DELAY_MS } from "@/ui-kit/primitives/hover-card";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import { mediaDeliverySettingsFixture } from "@/test-fixtures/media-delivery";
import type { PublicAiSettingsSnapshot, ProviderSummary } from "@/types";
import type { ComposerProviderToolSettingsSnapshot } from "./composer-model";
import { ModelPickerControl, modelPickerChoicesFromProviders, type ModelPickerChoice } from "./model-picker-control";
import { ModelReasoningPickerControl, describeReasoningSelection } from "./model-reasoning-picker-control";
import { ComposerAiCapabilityControl, ContextUsageOrb } from "./composer-capability-controls";
import { PendingPromptQueueStrip } from "./composer-prompt-controls";
import { DedicatedComposerContext } from "./dedicated/dedicated-session-controls";
import type { DedicatedMemorySelection } from "@/service/dedicated-memory";
import { useComposerPlanMode } from "./composer-panel/use-composer-plan-mode";
import { ComposerPanelToolbarRight } from "./composer-panel/toolbar-right";
import { loadPublicAiSettings, loadPublicAiSessionProviderToolSelections, queryPublicAiConnectionUsage, savePublicAiSessionProviderToolSelections } from "@/service/settings-actions-service";

// translate keeps callback identity stable while exposing exact semantic labels.
// translate 保持回调身份稳定，同时暴露精确语义标签。
const translate = (key: string) => key;
vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "en-US", t: translate }) }));
vi.mock("@/service/settings-actions-service", () => ({
  loadPublicAiSettings: vi.fn(), loadPublicAiSessionProviderToolSelections: vi.fn(),
  savePublicAiSessionProviderToolSelections: vi.fn(), rebuildPublicAiSessionProviderToolSelections: vi.fn(),
  queryPublicAiConnectionUsage: vi.fn(),
}));
// root and host own only the isolated component test document.
// root 与 host 仅持有隔离组件测试文档。
let root: Root;
let host: HTMLDivElement;
// The account card renders usage through a lazily loaded module. Loading it once up front makes every
// later React.lazy read resolve synchronously, so assertions follow component state instead of how fast
// a dynamic import happens to finish under a busy test run.
// 账号卡片通过懒加载模块渲染用量。提前加载一次，使之后每次 React.lazy 读取都同步完成，
// 让断言跟随组件状态，而不取决于繁忙测试运行中动态导入恰好多快完成。
beforeAll(async () => { await import("@/components/public-ai/credential-usage-overview"); });
beforeEach(() => {
  vi.clearAllMocks(); vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  Element.prototype.scrollIntoView = vi.fn();
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

// render mounts real shared primitives with a tooltip context; element is the subject.
// render 在提示上下文中挂载真实共享原语；element 是测试对象。
async function render(element: React.ReactNode) { await act(async () => root.render(<TooltipProvider>{element}</TooltipProvider>)); }
// press delivers a real bubbling key event; returns whether the browser default was prevented.
// press 分派真实冒泡键盘事件；返回是否阻止浏览器默认行为。
async function press(element: Element, key: string) {
  const event = new KeyboardEvent("keydown", { key, code: key === " " ? "Space" : key, bubbles: true, cancelable: true });
  await act(async () => { element.dispatchEvent(event); await new Promise((resolve) => setTimeout(resolve, 1)); });
  return event.defaultPrevented;
}
// click resolves the exact accessible label and dispatches the native click.
// click 解析精确无障碍标签并分派原生点击。
async function click(label: string) {
  await act(async () => { document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!.click(); });
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 1)); });
}
// providers declares two selectable model routes for keyboard navigation and account search.
// providers 声明两条可选择模型路由，用于键盘导航与账号搜索。
function providers(): ProviderSummary[] {
  return [{ id: "a", sourceProviderId: "p", connectionId: "a", accountName: "Account A", name: "Provider", status: "ready",
    models: [{ id: "first", name: "First", active: true }, { id: "second", name: "Second", active: false }] }];
}

it("selects the second exact model with ArrowDown and Enter through real cmdk", async () => {
  const select = vi.fn();
  await render(<ModelPickerControl ariaLabel="Models" fallbackLabel="Pick" filterPlaceholder="Find" noMatchesLabel="Empty"
    providers={providers()} value={JSON.stringify(["p", "first", "a"])} onValueChange={select} />);
  await click("Models");
  const input = document.querySelector<HTMLInputElement>('input[aria-label="Find"]')!;
  await press(input, "ArrowDown"); await press(input, "Enter");
  expect(select).toHaveBeenCalledWith(expect.objectContaining({ modelId: "second", connectionId: "a" }));
  expect(document.querySelector('[role="listbox"]')).toBeNull();
});

it("keeps Tab available in the search field and dismisses with Escape", async () => {
  await render(<ModelPickerControl ariaLabel="Models" fallbackLabel="Pick" filterPlaceholder="Find" noMatchesLabel="Empty"
    providers={providers()} value="" onValueChange={vi.fn()} />);
  await click("Models");
  const input = document.querySelector<HTMLInputElement>('input[aria-label="Find"]')!;
  expect(await press(input, "Tab")).toBe(false);
  await click("Models"); await press(document.querySelector<HTMLInputElement>("input[aria-label=Find]")!, "Escape");
  expect(document.querySelector('[role="listbox"]')).toBeNull();
});

// MergedHarness mounts the real merged model/account/reasoning control with explicit local callbacks.
// MergedHarness 使用显式本地回调挂载真实的模型/账号/思考合并控件。
function MergedHarness(props: {
  accounts?: ModelPickerChoice[];
  accountValue?: string;
  options?: string[];
  effort?: string;
  defaultEffort?: string | null;
  onAccountChange?: (choice: ModelPickerChoice) => void;
  onModelChange?: (choice: ModelPickerChoice) => void;
  onEffortChange?: (value: string) => void;
  modelSignal?: number;
}) {
  return (
    <ModelReasoningPickerControl
      accountAriaLabel="Account"
      accounts={props.accounts ?? modelPickerChoicesFromProviders(providers()).slice(0, 1)}
      accountValue={props.accountValue ?? "a"}
      ariaLabel="Model"
      defaultReasoningEffort={props.defaultEffort ?? null}
      fallbackLabel="Pick"
      filterPlaceholder="Find"
      modelOpenSignal={props.modelSignal}
      noMatchesLabel="Empty"
      onAccountChange={props.onAccountChange ?? vi.fn()}
      onModelChange={props.onModelChange ?? vi.fn()}
      onReasoningEffortChange={props.onEffortChange ?? vi.fn()}
      providers={providers()}
      reasoningEffort={props.effort ?? ""}
      reasoningOptions={(props.options ?? []).map((value) => ({ value, label: value }))}
      value={JSON.stringify(["p", "first", "a"])}
    />
  );
}

it("classifies reasoning options into a left-to-right scale or one explained fixed state", () => {
  const options = (...values: string[]) => values.map((value) => ({ value, label: `raw-${value}` }));
  expect(describeReasoningSelection(options("none", "low", "high"), "high", translate as never))
    .toEqual({ kind: "scale", index: 2, label: "composer.reasoning.level.high" });
  expect(describeReasoningSelection(options("none", "adaptive"), "gone", translate as never))
    .toEqual({ kind: "scale", index: -1, label: null });
  expect(describeReasoningSelection(options("fixed-on"), "fixed-on", translate as never)).toMatchObject({ kind: "fixed", tone: "on" });
  expect(describeReasoningSelection(options("fixed-off"), "fixed-off", translate as never)).toMatchObject({ kind: "fixed", tone: "off" });
  expect(describeReasoningSelection(options("adaptive"), "adaptive", translate as never)).toMatchObject({ kind: "fixed", tone: "adaptive" });
  expect(describeReasoningSelection(options("off"), "off", translate as never)).toMatchObject({ kind: "fixed", tone: "default" });
  expect(describeReasoningSelection(options("future"), "future", translate as never))
    .toEqual({ kind: "fixed", tone: "level", label: "raw-future" });
  expect(describeReasoningSelection([], "", translate as never)).toEqual({ kind: "fixed", tone: "unavailable", label: null });
  for (const unset of ["", "off"]) {
    expect(describeReasoningSelection(options("none", "low", "high"), unset, translate as never, "low"))
      .toEqual({ kind: "scale", index: 1, label: "composer.reasoning.level.low" });
  }
});

it("opens on the reasoning pane, returns there after a model choice and restores the model default", async () => {
  const changeModel = vi.fn();
  const changeEffort = vi.fn();
  await render(<MergedHarness defaultEffort="low" effort="high" onEffortChange={changeEffort} onModelChange={changeModel} options={["none", "low", "high"]} />);
  await click("Model");
  const slider = document.querySelector<HTMLElement>('[role="slider"]')!;
  expect(document.activeElement).toBe(slider);
  await press(slider, "ArrowLeft");
  expect(changeEffort).toHaveBeenLastCalledWith("low");
  await click("composer.reasoning.restoreDefault");
  expect(changeEffort).toHaveBeenCalledTimes(2);
  await click("composer.modelReasoning.switchModel");
  const input = document.querySelector<HTMLInputElement>('input[aria-label="Find"]')!;
  await press(input, "ArrowDown"); await press(input, "Enter");
  expect(changeModel).toHaveBeenCalledWith(expect.objectContaining({ modelId: "second" }));
  expect(document.querySelector('input[aria-label="Find"]')).toBeNull();
  expect(document.querySelector('[role="slider"]')).not.toBeNull();
});

it("opens the model list directly from the slash signal and closes it with Escape", async () => {
  await render(<MergedHarness modelSignal={0} options={["low", "high"]} />);
  await render(<MergedHarness modelSignal={1} options={["low", "high"]} />);
  const input = document.querySelector<HTMLInputElement>('input[aria-label="Find"]')!;
  expect(input).not.toBeNull();
  expect(document.querySelector(".vk-mr-back")).toBeNull();
  await press(input, "Escape");
  expect(document.querySelector(".vk-mr-pop")).toBeNull();
});

it("asks for an account instead of displaying the first account for a missing exact credential", async () => {
  const choose = vi.fn();
  vi.mocked(loadPublicAiSettings).mockResolvedValue(settings());
  await render(<MergedHarness accountValue="deleted" onAccountChange={choose} />);
  expect(host.querySelector(".vk-pick-alert")).not.toBeNull();
  await click("Model");
  const row = document.querySelector<HTMLButtonElement>('[aria-label="Account"]')!;
  expect(row.textContent).not.toContain("Account A");
  expect(row.textContent).toContain("composer.modelPicker.selectAccount");
  await click("Account");
  const option = document.querySelector<HTMLButtonElement>(".vk-mr-account")!;
  expect(option.textContent).toContain("Account A");
  await act(async () => option.click());
  expect(choose).toHaveBeenCalledWith(expect.objectContaining({ connectionId: "a" }));
});

// The trigger hover card only displays the current state; the click-opened popover replaces it.
// 触发器悬浮卡片只展示当前状态；点击打开的浮层会取代它。
it("shows a read-only model, reasoning and plan usage summary until the popover opens", async () => {
  vi.mocked(loadPublicAiSettings).mockResolvedValue(settings());
  await render(<MergedHarness effort="low" options={["none", "low", "high"]} />);
  await act(async () => { document.querySelector<HTMLButtonElement>('[aria-label="Model"]')!.focus(); });
  await act(async () => { await new Promise(resolve => setTimeout(resolve, HOVER_CARD_OPEN_DELAY_MS + 50)); });
  // summary is the mounted hover card; it must not contain any interactive selection control.
  // summary 是已挂载的悬浮卡片；其中不得包含任何交互式选择控件。
  const summary = document.querySelector(".vk-mr-summary");
  expect(summary?.textContent).toContain("First");
  expect(summary?.textContent).toContain("composer.reasoning.level.low");
  expect(summary?.textContent).toContain("composer.modelReasoning.planUsage");
  expect(summary?.querySelectorAll(".vk-mr-pips > span[data-on]")).toHaveLength(2);
  expect(summary?.querySelector('button, [role="slider"], input')).toBeNull();
  // One read resolves the cache lifetime on mount; the account pane adds exactly one more.
  // 挂载时一次读取解析缓存时长；账号面板只再增加一次。
  expect(loadPublicAiSettings).toHaveBeenCalledTimes(2);
  expect(queryPublicAiConnectionUsage).not.toHaveBeenCalled();
  await click("Model");
  expect(document.querySelector(".vk-mr-summary")).toBeNull();
  expect(document.querySelector('[role="slider"]')).not.toBeNull();
});

// Missing credential-profile metadata cannot authorize a provider-facing usage query.
// 缺失凭据档案元数据不能授权面向供应商的用量查询。
it("does not offer usage refresh without the declared credential profile", async () => {
  // snapshot deliberately retains a connection whose owning provider directory is absent.
  // snapshot 明确保留一条所属供应商目录缺失的连接。
  const snapshot = settings();
  snapshot.connections = [{ id: "a", provider_id: "p", service_entry_id: "entry", credential_profile_id: "profile",
    billing_kind: "pay_as_you_go", usage_capability: "unsupported", usage_requires_management_key: false,
    has_management_key: false, credential_mode: "token", credential_id: "synthetic", display_name: "Account A",
    enabled: true, pending_validation: false }];
  vi.mocked(loadPublicAiSettings).mockResolvedValue(snapshot);
  await render(<MergedHarness />);
  await click("Model");
  // One read resolves the cache lifetime on mount; the account pane adds exactly one more.
  // 挂载时一次读取解析缓存时长；账号面板只再增加一次。
  expect(loadPublicAiSettings).toHaveBeenCalledTimes(2);
  await click("Account");
  expect(document.querySelector(".vk-account-card")?.textContent).toContain("Account A");
  expect(document.querySelector('[aria-label="composer.accountCard.refresh"]')).toBeNull();
});

// Account changes invalidate the exact read owner even when its promise rejects later.
// 即使请求稍后才失败，切换账号也会使精确读取所有者失效。
it("ignores an account detail failure after switching to another account", async () => {
  // rejectRead owns the delayed response from account a; account b must not inherit its error.
  // rejectRead 持有账号 a 的延迟响应；账号 b 不得继承该错误。
  let rejectRead!: (reason: Error) => void;
  vi.mocked(loadPublicAiSettings).mockResolvedValueOnce(settings())
    .mockReturnValueOnce(new Promise((_, reject) => { rejectRead = reject; }))
    .mockReturnValue(new Promise(() => {}));
  // accounts contains two eligible identities for the same explicit model.
  // accounts 包含同一明确模型的两个可用身份。
  const accounts = modelPickerChoicesFromProviders(providers()).slice(0, 1);
  accounts.push({ ...accounts[0]!, connectionId: "b", accountName: "Account B" });
  await render(<MergedHarness accounts={accounts} />);
  await click("Model");
  await click("Account");
  expect(loadPublicAiSettings).toHaveBeenCalledTimes(2);
  await render(<MergedHarness accounts={accounts} accountValue="b" />);
  await act(async () => rejectRead(new Error("old account failure")));
  expect(document.querySelector(".vk-account-card")?.textContent).toContain("Account B");
  expect(document.querySelector('.vk-mr-account[aria-pressed="true"]')?.textContent).toContain("Account B");
  expect(document.body.textContent).not.toContain("old account failure");
});

// Only an explicit refresh queries the provider, with exact identity, deduplication and retry after failure.
// 只有显式刷新才查询供应商，并保持精确身份、去重与失败后重试。
it("refreshes declared account usage once and recovers from an explicit failure", async () => {
  // snapshot declares a complete synthetic credential profile supporting a balance query.
  // snapshot 声明一个完整的合成凭据档案，支持余额查询。
  const snapshot = settings();
  snapshot.connections = [{ id: "a", provider_id: "p", service_entry_id: "entry", credential_profile_id: "profile",
    billing_kind: "pay_as_you_go", usage_capability: "deep_seek_balance", usage_requires_management_key: false,
    has_management_key: false, credential_mode: "token", credential_id: "synthetic", display_name: "Account A",
    enabled: true, pending_validation: false }];
  snapshot.providers = [{ id: "p", display_name: "Provider", kind: "custom", category: "unclassified",
    credential_directory_visibility: "configured_only", credential_registration: "direct", service_entries: [{
      id: "entry", display_name: "Entry", kind: "conversation", base_url: "https://audit.invalid/v1",
      credential_profiles: [{ id: "profile", display_name: "Profile", credential_mode: "token", billing_kind: "pay_as_you_go", account_usage_query: "deep_seek_balance" }],
      authentication: "bearer_authorization", model_discovery: "manual", protocol_routes: [],
    }] }];
  vi.mocked(loadPublicAiSettings).mockResolvedValue(snapshot);
  await render(<MergedHarness />);
  await click("Model");
  await click("Account");
  expect(queryPublicAiConnectionUsage).not.toHaveBeenCalled();
  // rejectRefresh retains the sole in-flight provider request until the explicit failure arrives.
  // rejectRefresh 保留唯一在途供应商请求，直到显式失败到达。
  let rejectRefresh!: (reason: Error) => void;
  vi.mocked(queryPublicAiConnectionUsage).mockReturnValue(new Promise((_, reject) => { rejectRefresh = reject; }));
  // refresh is the actual enabled action in the mounted account pane.
  // refresh 是已挂载账号面板中的实际启用操作。
  const refresh = document.querySelector<HTMLButtonElement>('[aria-label="composer.accountCard.refresh"]')!;
  await act(async () => { refresh.click(); refresh.click(); });
  expect(queryPublicAiConnectionUsage).toHaveBeenCalledExactlyOnceWith("a");
  await act(async () => rejectRefresh(new Error("explicit usage failure")));
  expect(document.body.textContent).toContain("explicit usage failure");
  vi.mocked(queryPublicAiConnectionUsage).mockResolvedValue({ credential_id: "synthetic", availability: "supported", balance_amount: "17.25", currency: "USD", quota_windows: [] });
  await act(async () => refresh.click());
  expect(document.body.textContent).not.toContain("explicit usage failure");
  expect(document.body.textContent).toContain("17.25");
  // Escape steps back to the reasoning pane, whose account row now summarizes the observed balance.
  // Escape 返回思考面板，其账号行此时概括已观测余额。
  await press(document.activeElement ?? document.body, "Escape");
  expect(document.querySelector(".vk-account-card")).toBeNull();
  expect(document.querySelector('[aria-label="Account"]')?.textContent).toContain("17.25");
});

// Model menus must discard an open state when their owner disables interaction.
// 模型菜单必须在所有者禁用交互时丢弃已打开状态。
it("does not reopen an old model menu after a disabled interval", async () => {
  // draw preserves the real picker instance while changing only host availability and slash intent.
  // draw 保留真实选择器实例，仅改变宿主可用性与斜杠意图。
  const draw = (disabled: boolean, openSignal: number, hasModels = true) => render(<ModelPickerControl ariaLabel="Models"
    fallbackLabel="Pick" filterPlaceholder="Find" noMatchesLabel="Empty" providers={hasModels ? providers() : []}
    value="" onValueChange={vi.fn()} disabled={disabled} openSignal={openSignal} />);
  await draw(false, 0); await click("Models");
  expect(document.querySelector('[role="listbox"]')).not.toBeNull();
  await draw(true, 0); await draw(false, 0);
  expect(document.querySelector('[role="listbox"]')).toBeNull();
  await draw(true, 1); await draw(false, 1);
  expect(document.querySelector('[role="listbox"]')).toBeNull();
  await draw(false, 2);
  expect(document.querySelector('[role="listbox"]')).not.toBeNull();
  await draw(false, 2, false); await draw(false, 2);
  expect(document.querySelector('[role="listbox"]')).toBeNull();
});

// PlanHarness exposes the real capture handler beside a draft input and a toolbar control.
// PlanHarness 在草稿输入与工具栏控件旁暴露真实捕获处理器。
function PlanHarness() {
  const plan = useComposerPlanMode({ scopeKey: "draft", conversationId: null, sessionPlanMode: null, goal: null, onSetSessionPlanMode: vi.fn() });
  return <section onKeyDownCapture={plan.handleContainerKeyDown}><textarea /><input /><output>{String(plan.planModeEnabled)}</output></section>;
}
it("reserves Shift+Tab for the draft instead of intercepting selector keyboard navigation", async () => {
  await render(<PlanHarness />);
  const inputEvent = new KeyboardEvent("keydown", { key: "Tab", shiftKey: true, bubbles: true, cancelable: true });
  await act(async () => { host.querySelector("input")!.dispatchEvent(inputEvent); });
  expect(inputEvent.defaultPrevented).toBe(false); expect(host.querySelector("output")!.textContent).toBe("false");
  await act(async () => { host.querySelector("textarea")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", shiftKey: true, bubbles: true, cancelable: true })); });
  expect(host.querySelector("output")!.textContent).toBe("true");
});

it("reorders pending entries with Space, ArrowDown and Space using the real keyboard sensor", async () => {
  const reorder = vi.fn();
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const rows = [...document.querySelectorAll(".vk-queue-row")];
    const index = Math.max(0, rows.indexOf(this));
    return new DOMRect(0, index * 50, 300, 40);
  });
  await render(<PendingPromptQueueStrip entries={["first", "second"].map((id) => ({ id, prompt: id, preview: id, attachments: [], state: "ready", createdAt: "0" }))}
    onEdit={vi.fn()} onInsert={vi.fn()} onRemove={vi.fn()} onReorder={reorder} />);
  const handle = document.querySelector<HTMLElement>(".vk-queue-handle")!;
  handle.focus(); await press(handle, " "); await press(handle, "ArrowDown"); await press(handle, " ");
  expect(reorder).toHaveBeenCalledExactlyOnceWith(["second", "first"], "first", null);
});

it("opens context controls with a real popover and preserves node selection on rejected writes", async () => {
  let reject!: (reason: Error) => void;
  const change = vi.fn(() => new Promise<boolean>((_, fail) => { reject = fail; }));
  await render(<ContextUsageOrb snapshot={{ baselineMatchesActiveRuntime: true, breakdown: [], cacheHitBasisPoints: null,
    maxTokens: 950_000, percent: 8, severity: "normal", tooltipRows: [], usageBasisPoints: 800, usedTokens: 76_000 }}
    contextCompactionWindowNodes={settings().contextCompactionWindowNodes.nodes} selection={null} customMinTokens={100_000} modelContextLimit={950_000} onSelectionChange={change} />);
  await click("contextUsage.title");
  const node = document.querySelector<HTMLButtonElement>('[role="radio"][value="compression-window-1"]')!;
  await act(async () => { node.click(); node.click(); });
  expect(change).toHaveBeenCalledExactlyOnceWith({ node: "compression-window-1" });
  await act(async () => reject(new Error("node write failed")));
  expect(document.querySelector('[role="alert"]')?.textContent).toBe("composer.contextWindow.saveFailed");
  expect(document.querySelector("details")?.open).toBe(false);
  expect(document.querySelector("details pre")?.textContent).toBe("node write failed");
  expect(document.querySelector('[role="radio"][value="default"]')?.getAttribute("aria-checked")).toBe("true");
});

// Repeated explicit selection must reach the service even when the controlled value is unchanged.
// 即使受控值不变，重复显式选择也必须到达服务。
it.each([null, { node: "compression-window-1" }] as const)("resubmits the selected compaction window %o and coalesces only pending writes", async (selection) => {
  // finishWrite settles the current service acknowledgement without changing the selected value.
  // finishWrite 结束当前服务确认，而不改变已选值。
  let finishWrite!: (saved: boolean) => void;
  // change records the exact idempotent mutation emitted by the real selector.
  // change 记录真实选择器发出的精确幂等修改。
  const change = vi.fn(() => new Promise<boolean>((resolve) => { finishWrite = resolve; }));
  await render(<ContextUsageOrb snapshot={{ baselineMatchesActiveRuntime: true, breakdown: [], cacheHitBasisPoints: null,
    maxTokens: 950_000, percent: 8, severity: "normal", tooltipRows: [], usageBasisPoints: 800, usedTokens: 76_000 }}
    contextCompactionWindowNodes={settings().contextCompactionWindowNodes.nodes} selection={selection} customMinTokens={100_000}
    modelContextLimit={950_000} onSelectionChange={change} />);
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-ctx-button")!.click());
  expect(change).not.toHaveBeenCalled();
  // selected resolves by state, so catalog insertion or reordering does not change the test target.
  // selected 按状态定位，因此目录插入或重排不会改变测试对象。
  const selected = document.querySelector<HTMLButtonElement>('[role="radio"][aria-checked="true"]')!;
  await act(async () => { selected.click(); selected.click(); });
  expect(change).toHaveBeenCalledExactlyOnceWith(selection);
  expect(selected.disabled).toBe(true);
  await act(async () => finishWrite(true));
  expect(selected.disabled).toBe(false);
  await act(async () => selected.click());
  expect(change).toHaveBeenCalledTimes(2);
  expect(change).toHaveBeenLastCalledWith(selection);
  await act(async () => finishWrite(true));
  expect(selected.getAttribute("aria-checked")).toBe("true");
  expect(document.querySelector('[role="alert"]')).toBeNull();
});

// CapacityToolbar mounts the production owner boundary and capacity selector with complete synthetic props.
// CapacityToolbar 使用完整合成参数挂载生产所有者边界及容量选择器。
function CapacityToolbar({ owner, change }: { owner: string; change: React.ComponentProps<typeof ComposerPanelToolbarRight>["onSetSessionContextCompactionWindow"] }) {
  return <ComposerPanelToolbarRight draftSlotKey={owner} attachmentCount={0} draftInput="" isConversationRunning={false} sendBlocked={false} stopPending={false}
    contextUsage={{ baselineMatchesActiveRuntime: true, breakdown: [], cacheHitBasisPoints: null, maxTokens: 950_000,
      percent: 8, severity: "normal", tooltipRows: [], usageBasisPoints: 800, usedTokens: 76_000 }}
    contextCompactionWindowNodes={settings().contextCompactionWindowNodes.nodes} sessionContextWindowSelection={null} customContextWindowMinTokens={100_000} modelContextLimit={950_000}
    onSetSessionContextCompactionWindow={change} onSend={vi.fn()} sendAriaLabel="Send" stopAriaLabel="Stop" />;
}

it("isolates pending capacity writes and open popovers by the actual toolbar draft owner", async () => {
  // rejectOld and resolveNew independently settle two different conversation writes.
  // rejectOld 与 resolveNew 独立结束两个不同会话的写入。
  let rejectOld!: (reason: Error) => void;
  let resolveNew!: (saved: boolean) => void;
  const oldChange = vi.fn(() => new Promise<boolean>((_, reject) => { rejectOld = reject; }));
  const newChange = vi.fn(() => new Promise<boolean>((resolve) => { resolveNew = resolve; }));
  await render(<CapacityToolbar owner="conversation:a" change={oldChange} />);
  await click("contextUsage.title");
  await act(async () => document.querySelector<HTMLButtonElement>('[role="radio"][value="compression-window-1"]')!.click());
  await render(<CapacityToolbar owner="conversation:b" change={newChange} />);
  expect(document.querySelector('[data-slot="popover-content"]')).toBeNull();
  expect(document.querySelector('[role="listbox"]')).toBeNull();
  await click("contextUsage.title");
  // node belongs to the new owner's popover and must accept an independent write.
  // node 属于新所有者的浮层，必须能够接受独立写入。
  const node = document.querySelector<HTMLButtonElement>('[role="radio"][value="compression-window-1"]')!;
  expect(node.disabled).toBe(false);
  await act(async () => node.click());
  await act(async () => rejectOld(new Error("old owner failure")));
  expect(node.disabled).toBe(true);
  expect(document.querySelector('[role="alert"]')).toBeNull();
  await act(async () => resolveNew(true));
  expect(node.disabled).toBe(false);
  expect(oldChange).toHaveBeenCalledExactlyOnceWith({ node: "compression-window-1" });
  expect(newChange).toHaveBeenCalledExactlyOnceWith({ node: "compression-window-1" });
});

it("shows a concise failed-save status for a false acknowledgement and allows retry", async () => {
  // change follows the production action contract: false rejects without throwing.
  // change 遵循生产动作契约：false 表示拒绝，不抛出异常。
  const change = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
  await render(<CapacityToolbar owner="conversation:false" change={change} />);
  await click("contextUsage.title");
  // node remains bound to the last confirmed selected value until the parent publishes success.
  // node 在父级发布成功前继续绑定最后确认的选择值。
  const node = document.querySelector<HTMLButtonElement>('[role="radio"][value="compression-window-1"]')!;
  await act(async () => node.click());
  expect(document.querySelector('[role="alert"]')?.textContent).toBe("composer.contextWindow.saveFailed");
  expect(document.querySelector('details')).toBeNull();
  expect(node.disabled).toBe(false);
  await act(async () => node.click());
  expect(change).toHaveBeenCalledTimes(2);
  expect(document.querySelector('[role="alert"]')).toBeNull();
});

it("retains an invalid saved node and lets the user restore the exact model default", async () => {
  // change records explicit reset; loading this state must never rewrite the saved node automatically.
  // change 记录显式重置；加载此状态绝不能自动改写已保存节点。
  const change = vi.fn().mockResolvedValue(true);
  await render(<ContextUsageOrb snapshot={{ baselineMatchesActiveRuntime: true, breakdown: [], cacheHitBasisPoints: 0,
    maxTokens: 256_000, percent: 0, severity: "normal", tooltipRows: [], usageBasisPoints: 0, usedTokens: 0 }}
    contextCompactionWindowNodes={settings().contextCompactionWindowNodes.nodes} selection={{ node: "compression-window-5" }} customMinTokens={100_000} modelContextLimit={256_000} onSelectionChange={change} />);
  await act(async () => host.querySelector<HTMLButtonElement>('.vk-ctx-button')!.click());
  expect(change).not.toHaveBeenCalled();
  expect(document.querySelector<HTMLButtonElement>('[role="radio"][value="invalid"]')?.disabled).toBe(true);
  // A node equal to the model window duplicates the default item; a larger one does not fit.
  // 等于模型窗口的节点与默认项重复；更大的节点放不下。
  expect(document.querySelector('[role="radio"][value="compression-window-1"]')).not.toBeNull();
  expect(document.querySelector('[role="radio"][value="compression-window-2"]')).toBeNull();
  expect(document.querySelector('[role="radio"][value="compression-window-3"]')).toBeNull();
  expect(document.querySelector('[data-ctx-cache] strong')?.textContent).toBe("0.0%");
  await act(async () => document.querySelector<HTMLButtonElement>('[role="radio"][value="default"]')!.click());
  expect(change).toHaveBeenCalledExactlyOnceWith(null);
});

it("validates a typed custom window before saving it", async () => {
  const change = vi.fn().mockResolvedValue(true);
  await render(<ContextUsageOrb snapshot={{ baselineMatchesActiveRuntime: true, breakdown: [], cacheHitBasisPoints: null,
    maxTokens: 400_000, percent: 0, severity: "normal", tooltipRows: [], usageBasisPoints: 0, usedTokens: 0 }}
    contextCompactionWindowNodes={settings().contextCompactionWindowNodes.nodes} selection={null} customMinTokens={100_000}
    modelContextLimit={400_000} onSelectionChange={change} />);
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-ctx-button")!.click());
  await act(async () => document.querySelector<HTMLButtonElement>('[role="radio"][value="custom"]')!.click());
  expect(change).not.toHaveBeenCalled();
  const input = document.querySelector<HTMLInputElement>(".vk-window-custom input")!;
  const form = document.querySelector<HTMLFormElement>(".vk-window-custom")!;
  // typeInto drives the controlled input through React's value tracker.
  // typeInto 通过 React 的值跟踪器驱动受控输入框。
  const typeInto = async (value: string) => act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await typeInto("50K");
  await act(async () => form.requestSubmit());
  expect(change).not.toHaveBeenCalled();
  expect(document.querySelector(".vk-window-custom [role=\"alert\"]")).not.toBeNull();
  await typeInto("401K");
  await act(async () => form.requestSubmit());
  expect(change).not.toHaveBeenCalled();
  await typeInto("235k");
  await act(async () => form.requestSubmit());
  expect(change).toHaveBeenCalledExactlyOnceWith({ custom: 235_000 });
  expect(document.querySelector(".vk-window-custom")).toBeNull();
});

// settings returns a complete synthetic catalog with one exact image-understanding target.
// settings 返回包含一条精确图片理解目标的完整合成目录。
function settings(): PublicAiSettingsSnapshot {
  return {
    manualModelDefaults: { contextLimit: 200_000, outputLimit: 32_000 },
    usageRefreshIntervalSeconds: 300, mediaDelivery: mediaDeliverySettingsFixture(), contextCompactionWindowNodes: { nodes: [
      { id: "compression-window-1", tokens: 200_000 }, { id: "compression-window-2", tokens: 256_000 },
      { id: "compression-window-3", tokens: 272_000 }, { id: "compression-window-4", tokens: 373_000 }, { id: "compression-window-5", tokens: 500_000 },
    ] },
    newSessionContextCompactionWindowNodeId: null, modelContextWindowDefaults: [], modelContextWindowCandidates: [], customContextWindowMinTokens: 100000,
    providerPriorities: {}, usageCache: {}, connections: [], providers: [], providerDefaults: {}, catalogs: {}, modelSupportCatalogs: {},
    catalogErrors: {}, sourceCatalogs: {}, catalogStatuses: {}, modelAccessRecords: [],
    providerToolTemplateModels: [], connectionModelAccessPolicyRecords: [], customModels: {},
    customProviderSystemModelDefaults: { context_limit: 200_000, output_limit: 32_768, input_resources: [], reasoning: "native", reasoning_control: "effort_selection", reasoning_efforts: ["high"] },
    modelCapabilityOverrides: {}, modelCapabilityOverrideRecords: [], providerToolTargets: [{ connectionId: "a", connectionDisplayName: "Account A", manifestSourceLayer: "bundled",
      target: { target_id: "image", provider_id: "p", service_entry_id: "entry", credential_profile_ids: ["profile"], upstream_target_id: "image", display_name: "Image model",
        contract: { tool_id: "image_understand", description: "Understand an image.", capability: "image_understand", input_schema: { type: "object" }, output: { kind: "text", source_path: "$.text", normalized_result_path: "$.text" } } } }],
    providerToolEnabledCapabilities: [], providerToolSelections: {}, mediaUnderstandingSelections: {}, mediaUnderstandingBackendOptions: {}, providerExtensionModes: {}, providerExtensionTargetSets: {},
    internalInferenceCandidates: [], inferenceProfiles: {}, inferenceRouteDiagnostics: [], internalModelOverrides: {}, contextWindowPolicyOverrides: {}, contextWindowPolicyDefaults: { effective_context_window_percent: 95, auto_compact_percent: 90, auto_compact_token_limit: null, auto_compact_scope: "total", fallback_buffer_tokens: 0 },
  };
}
// session constructs a complete session selection, optionally referencing a removed target.
// session 构造完整会话选择，可指定引用已移除目标。
function session(target = "image"): ComposerProviderToolSettingsSnapshot {
  return { enabledCapabilities: ["image_understand"], selections: {}, extensionTargetSets: {}, extensionModes: {},
    mediaUnderstandingSelections: { image_understand: { mode: "provider_tool", selection: { connection_id: "a", target_id: target } } } };
}
// openCapabilities supplies synthetic reads and opens the real capability dialog.
// openCapabilities 提供合成读取并打开真实能力弹窗。
async function openCapabilities(value = session(), catalog = settings()) {
  vi.mocked(loadPublicAiSettings).mockResolvedValue(catalog);
  vi.mocked(loadPublicAiSessionProviderToolSelections).mockResolvedValue({ ...value, conflicts: [], requestsStarted: false });
  await render(<ComposerAiCapabilityControl ariaLabel="Capabilities" conversationId="c" />);
  await click("Capabilities");
}
// selectCapabilityTab clicks the intent tab whose visible label is the given translation key.
// selectCapabilityTab 点击可见标签为指定翻译键的意图标签。
async function selectCapabilityTab(labelKey: string) {
  const tab = [...document.querySelectorAll<HTMLButtonElement>(".vk-cap-tabs button")]
    .find((button) => button.querySelector("span")?.textContent === labelKey);
  expect(tab, `tab ${labelKey} may have been renamed or regrouped`).toBeDefined();
  await act(async () => tab!.click());
}
// listedCapabilities returns the capability labels rendered in the visible list.
// listedCapabilities 返回可见列表中渲染的能力名称。
function listedCapabilities(): string[] {
  return [...document.querySelectorAll(".vk-cap-copy > span:first-child")].map((title) => title.textContent ?? "");
}

it("shows one intent tab at a time and restores the capability last viewed in a tab", async () => {
  // A saved image-generation set keeps the generation tab visible beside the understanding tab.
  // 已保存的图片生成集合让生成标签与理解标签同时可见。
  await openCapabilities({ ...session(), extensionTargetSets: { image_generate: [{ connection_id: "a", target_id: "generator" }] } });
  expect(listedCapabilities()).toContain("settings.aiUnderstandImage");
  expect(listedCapabilities()).not.toContain("settings.providerTools.imageGenerate");
  await selectCapabilityTab("settings.aiCapabilities.tab.generate");
  expect(listedCapabilities()).toContain("settings.providerTools.imageGenerate");
  expect(listedCapabilities()).not.toContain("settings.aiUnderstandImage");
  expect(document.querySelectorAll('.vk-cap-select[aria-pressed="true"]')).toHaveLength(1);
  await selectCapabilityTab("settings.aiCapabilities.tab.understand");
  expect(document.querySelector('.vk-cap-select[aria-pressed="true"]')?.textContent)
    .toContain("settings.aiUnderstandImage");
});

it("releases a pending send block when the composer learns the session is dedicated", async () => {
  vi.mocked(loadPublicAiSettings).mockReturnValue(new Promise(() => {}));
  vi.mocked(loadPublicAiSessionProviderToolSelections).mockReturnValue(new Promise(() => {}));
  // onConflictChange records every send-boundary report in order.
  // onConflictChange 按顺序记录每一次发送边界报告。
  const onConflictChange = vi.fn<(blocked: boolean) => void>();
  // dedicated models the event window revealing the profile after the composer mounted.
  // dedicated 模拟输入区挂载后事件窗口才揭示专职配置。
  const dedicated: DedicatedMemorySelection = { sessionId: "c", generation: 1, toolAccess: "none", basePrompt: "p", lockedSpans: [], openTurnId: null };
  const control = (value: DedicatedMemorySelection | null) => (
    <DedicatedComposerContext.Provider value={value}>
      <ComposerAiCapabilityControl ariaLabel="Capabilities" conversationId="c" onConflictChange={onConflictChange} />
    </DedicatedComposerContext.Provider>
  );
  await render(control(null));
  expect(onConflictChange).toHaveBeenLastCalledWith(true);
  await render(control(dedicated));
  expect(onConflictChange).toHaveBeenLastCalledWith(false);
  expect(document.querySelector('[aria-label="Capabilities"]')).toBeNull();
});

it("opens the capability dialog from the hover summary footer action", async () => {
  vi.mocked(loadPublicAiSettings).mockResolvedValue(settings());
  vi.mocked(loadPublicAiSessionProviderToolSelections).mockResolvedValue({ ...session(), conflicts: [], requestsStarted: false });
  await render(<ComposerAiCapabilityControl ariaLabel="Capabilities" conversationId="c" />);
  await act(async () => { document.querySelector<HTMLButtonElement>('[aria-label="Capabilities"]')!.focus(); });
  await act(async () => { await new Promise(resolve => setTimeout(resolve, HOVER_CARD_OPEN_DELAY_MS + 50)); });
  // configure is the footer action rendered by the mounted hover summary.
  // configure 是已挂载悬浮摘要渲染的底部操作。
  const configure = document.querySelector<HTMLButtonElement>(".vk-cap-sum-action");
  expect(configure?.textContent).toBe("composer.capability.tooltipTitle");
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  await act(async () => configure!.click());
  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("composer.capability.dialogTitle");
  expect(document.querySelector(".vk-cap-sum-action")).toBeNull();
});

it("keeps both empty media modes and disables a stale enabled route even with candidates", async () => {
  await openCapabilities(session("removed"));
  const checkbox = document.querySelector<HTMLInputElement>('.vk-cap-row[data-selected] input[type="checkbox"]')!;
  expect(checkbox.checked).toBe(false); expect(checkbox.disabled).toBe(true);
  expect(document.querySelectorAll('[role="radio"]')).toHaveLength(2);
  expect(document.querySelector('[role="combobox"][aria-label="settings.providerTools.model"]')).not.toBeNull();
  expect(savePublicAiSessionProviderToolSelections).not.toHaveBeenCalled();
});

it("switches to an empty mode without enabling again after a valid target returns", async () => {
  await openCapabilities();
  await act(async () => document.querySelector<HTMLButtonElement>('[role="radio"][value="model_replacement"]')!.click());
  expect(document.querySelector<HTMLInputElement>('.vk-cap-row[data-selected] input[type="checkbox"]')!.checked).toBe(false);
  expect(document.querySelector<HTMLButtonElement>('[role="combobox"][aria-label="settings.providerTools.model"]')!.disabled).toBe(true);
  await act(async () => document.querySelector<HTMLButtonElement>('[role="radio"][value="provider_tool"]')!.click());
  expect(document.querySelector<HTMLInputElement>('.vk-cap-row[data-selected] input[type="checkbox"]')!.checked).toBe(false);
  expect(document.querySelector<HTMLInputElement>('.vk-cap-row[data-selected] input[type="checkbox"]')!.disabled).toBe(false);
});

it("blocks duplicate saves and Escape while pending, then keeps the draft on failure", async () => {
  await openCapabilities();
  let reject!: (reason: Error) => void;
  vi.mocked(savePublicAiSessionProviderToolSelections).mockReturnValue(new Promise((_, fail) => { reject = fail; }));
  await act(async () => document.querySelector<HTMLInputElement>('.vk-cap-row[data-selected] input[type="checkbox"]')!.click());
  const save = [...document.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === "composer.capability.save")!;
  await act(async () => { save.click(); save.click(); });
  expect(savePublicAiSessionProviderToolSelections).toHaveBeenCalledTimes(1);
  await press(document.querySelector('[role="dialog"]')!, "Escape");
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  await act(async () => reject(new Error("save failed")));
  expect(document.querySelector('[role="alert"]')?.textContent).toBe("composer.capability.operationFailed");
  expect(document.querySelector('details pre')?.textContent).toBe("save failed");
  expect(document.querySelector('details')?.open).toBe(false);
  expect(document.querySelector<HTMLInputElement>('.vk-cap-row[data-selected] input[type="checkbox"]')!.checked).toBe(false);
});

it("keeps a removed extension category recoverable and saves only the explicit removal", async () => {
  // configured owns a stale extension beside an unrelated valid media route.
  // configured 在无效扩展旁保存一条无关的有效媒体路由。
  const configured = { ...session(), extensionModes: { image_generate: { mode: "multi" as const } }, extensionTargetSets: { image_generate: [{ connection_id: "removed-account", target_id: "removed-model" }] } };
  await openCapabilities(configured);
  await selectCapabilityTab("settings.aiCapabilities.tab.generate");
  // category must remain visible even when the entire extension family has left the catalog.
  // 即使整个扩展能力族已离开目录，category 也必须保持可见。
  const category = [...document.querySelectorAll<HTMLButtonElement>(".vk-cap-select")]
    .find((button) => button.textContent?.includes("settings.providerTools.imageGenerate"));
  expect(category).toBeDefined();
  await act(async () => category!.click());
  await click("composer.capability.removeUnavailable · removed-model");
  expect(document.activeElement).toBe(category);
  vi.mocked(savePublicAiSessionProviderToolSelections).mockResolvedValue({ ...configured, extensionTargetSets: { image_generate: [] }, conflicts: [], requestsStarted: false });
  await act(async () => [...document.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent === "composer.capability.save")!.click());
  expect(savePublicAiSessionProviderToolSelections).toHaveBeenCalledExactlyOnceWith({
    conversationId: "c", ...configured, extensionTargetSets: { image_generate: [] },
  });
});

it("restores the global defaults into the draft and saves them only on explicit save", async () => {
  await openCapabilities();
  // button finds a footer action by its exact translated label.
  // button 按精确翻译标签查找底部操作。
  const button = (label: string) => [...document.querySelectorAll<HTMLButtonElement>("button")]
    .find((candidate) => candidate.textContent === label)!;
  const restore = button("composer.capability.restoreDefaults");
  expect(restore.disabled).toBe(false);
  await act(async () => restore.click());
  expect(savePublicAiSessionProviderToolSelections).not.toHaveBeenCalled();
  expect(document.querySelector<HTMLInputElement>('.vk-cap-row[data-selected] input[type="checkbox"]')!.checked).toBe(false);
  expect(document.querySelector(".vk-cap-foot-hint")?.textContent).toBe("composer.capability.saveHint.unsaved");
  expect(restore.disabled).toBe(true);
  // defaults mirrors the empty global configuration of the synthetic catalog.
  // defaults 对应合成目录中为空的全局配置。
  const defaults = { enabledCapabilities: [], selections: {}, extensionTargetSets: {}, extensionModes: {}, mediaUnderstandingSelections: {} };
  vi.mocked(savePublicAiSessionProviderToolSelections).mockResolvedValue({ ...defaults, conflicts: [], requestsStarted: false });
  await act(async () => button("composer.capability.save").click());
  expect(savePublicAiSessionProviderToolSelections).toHaveBeenCalledExactlyOnceWith({ conversationId: "c", ...defaults });
});

it("requires manual activation after replacing a stale media target", async () => {
  await openCapabilities(session("removed"));
  await click("settings.providerTools.model");
  // option selects the actual cmdk entry instead of directly mutating component state.
  // option 选择真实 cmdk 条目，而非直接修改组件状态。
  const option = [...document.querySelectorAll<HTMLElement>('[role="option"]')]
    .find((item) => item.textContent?.includes("Image model"))!;
  await act(async () => option.click());
  const checkbox = document.querySelector<HTMLInputElement>('.vk-cap-row[data-selected] input[type="checkbox"]')!;
  expect(checkbox.checked).toBe(false);
  expect(checkbox.disabled).toBe(false);
});

it("allows a stale media switch to be explicitly cleared without deleting its saved route", async () => {
  const configured = session("removed");
  await openCapabilities(configured);
  await click("composer.capability.disableUnavailable");
  expect(document.activeElement).toBe(document.querySelector('.vk-cap-select[aria-pressed="true"]'));
  vi.mocked(savePublicAiSessionProviderToolSelections).mockResolvedValue({ ...configured, enabledCapabilities: [], conflicts: [], requestsStarted: false });
  await act(async () => [...document.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent === "composer.capability.save")!.click());
  expect(savePublicAiSessionProviderToolSelections).toHaveBeenCalledExactlyOnceWith({
    conversationId: "c", ...configured, enabledCapabilities: [],
  });
});

it("removes only the unavailable exact account while preserving a selected native extension", async () => {
  const catalog = settings();
  // nativeTarget remains valid; a custom target is deliberately ineligible as a native extension.
  // nativeTarget 保持有效；自定义目标有意不具备原生扩展资格。
  const nativeTarget = { ...catalog.providerToolTargets[0], target: { ...catalog.providerToolTargets[0].target,
    contract: { ...catalog.providerToolTargets[0].target.contract, capability: "image_generate" as const } } };
  catalog.providerToolTargets = [nativeTarget, { ...nativeTarget, connectionId: "custom-account", manifestSourceLayer: "custom" }];
  const retained = { connection_id: "a", target_id: "image" };
  const configured = { ...session(), enabledCapabilities: [], extensionModes: { image_generate: { mode: "multi" as const } }, extensionTargetSets: { image_generate: [retained, { connection_id: "custom-account", target_id: "image" }] } };
  await openCapabilities(configured, catalog);
  await click("composer.capability.removeUnavailable · image");
  vi.mocked(savePublicAiSessionProviderToolSelections).mockResolvedValue({ ...configured, extensionTargetSets: { image_generate: [retained] }, conflicts: [], requestsStarted: false });
  await act(async () => [...document.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent === "composer.capability.save")!.click());
  expect(savePublicAiSessionProviderToolSelections).toHaveBeenCalledExactlyOnceWith({
    conversationId: "c", ...configured, extensionTargetSets: { image_generate: [retained] },
  });
});

it("does not apply an old conversation's pending save to the newly opened conversation", async () => {
  await openCapabilities();
  let finish!: (value: Awaited<ReturnType<typeof savePublicAiSessionProviderToolSelections>>) => void;
  vi.mocked(savePublicAiSessionProviderToolSelections).mockReturnValue(new Promise((resolve) => { finish = resolve; }));
  await act(async () => document.querySelector<HTMLInputElement>('.vk-cap-row[data-selected] input[type="checkbox"]')!.click());
  await act(async () => [...document.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent === "composer.capability.save")!.click());
  vi.mocked(loadPublicAiSessionProviderToolSelections).mockResolvedValue({ ...session(), conflicts: [], requestsStarted: false });
  await render(<ComposerAiCapabilityControl ariaLabel="Capabilities" conversationId="new-conversation" />);
  await act(async () => finish({ ...session(), enabledCapabilities: [], conflicts: [], requestsStarted: false }));
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  expect(document.querySelector<HTMLInputElement>('.vk-cap-row[data-selected] input[type="checkbox"]')!.checked).toBe(true);
  expect(savePublicAiSessionProviderToolSelections).toHaveBeenCalledTimes(1);
  expect(vi.mocked(savePublicAiSessionProviderToolSelections).mock.calls[0][0].conversationId).toBe("c");
});

it("clears stale content during a new conversation read and does not accept a late old read", async () => {
  let finish!: (value: Awaited<ReturnType<typeof loadPublicAiSessionProviderToolSelections>>) => void;
  vi.mocked(loadPublicAiSettings).mockResolvedValue(settings());
  vi.mocked(loadPublicAiSessionProviderToolSelections).mockReturnValue(new Promise((resolve) => { finish = resolve; }));
  await render(<ComposerAiCapabilityControl ariaLabel="Capabilities" conversationId="old" />);
  await click("Capabilities");
  vi.mocked(loadPublicAiSessionProviderToolSelections).mockResolvedValue({ ...session(), enabledCapabilities: [], conflicts: [], requestsStarted: false });
  await render(<ComposerAiCapabilityControl ariaLabel="Capabilities" conversationId="new" />);
  await act(async () => finish({ ...session("removed"), conflicts: [], requestsStarted: false }));
  expect(document.querySelector<HTMLInputElement>('.vk-cap-row[data-selected] input[type="checkbox"]')!.checked).toBe(false);
  expect(document.querySelector('[aria-invalid="true"]')).toBeNull();
  expect(savePublicAiSessionProviderToolSelections).not.toHaveBeenCalled();
});

it("keeps read failures compact and retries reads without overwriting the stored configuration", async () => {
  // failure is deliberately long; the primary alert must remain independent of its technical details.
  // failure 有意使用长内容；主提示必须独立于技术详情。
  const failure = "synthetic read rejected ".repeat(100);
  vi.mocked(loadPublicAiSettings).mockResolvedValue(settings());
  vi.mocked(loadPublicAiSessionProviderToolSelections).mockRejectedValue(new Error(failure));
  await render(<ComposerAiCapabilityControl ariaLabel="Capabilities" conversationId="c" />);
  await click("Capabilities");
  expect(document.querySelector('[role="alert"]')?.textContent?.trim()).toBe("composer.capability.loadFailed");
  expect(document.querySelector('details')?.open).toBe(false);
  expect(document.querySelector('details pre')?.textContent).toBe(failure);
  expect(document.querySelector('.vk-cap-select')).toBeNull();
  await act(async () => [...document.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent === "composer.capability.retry")!.click());
  expect(document.activeElement?.textContent).toBe("composer.capability.retry");
  vi.mocked(loadPublicAiSessionProviderToolSelections).mockResolvedValue({ ...session(), conflicts: [], requestsStarted: false });
  await act(async () => [...document.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent === "composer.capability.retry")!.click());
  expect(document.querySelector('[role="alert"]')).toBeNull();
  expect(document.activeElement).toBe(document.querySelector('.vk-cap-select[aria-pressed="true"]'));
  expect(document.querySelector<HTMLInputElement>('.vk-cap-row[data-selected] input[type="checkbox"]')!.checked).toBe(true);
  expect(savePublicAiSessionProviderToolSelections).not.toHaveBeenCalled();
});

it("keeps keyboard focus on the next unavailable extension after a removal", async () => {
  const configured = { ...session(), extensionModes: { image_generate: { mode: "multi" as const } }, extensionTargetSets: { image_generate: [
    { connection_id: "removed-account-1", target_id: "first" },
    { connection_id: "removed-account-2", target_id: "second" },
  ] } };
  await openCapabilities(configured);
  await selectCapabilityTab("settings.aiCapabilities.tab.generate");
  await act(async () => [...document.querySelectorAll<HTMLButtonElement>(".vk-cap-select")]
    .find((button) => button.textContent?.includes("settings.providerTools.imageGenerate"))!.click());
  await click("composer.capability.removeUnavailable · first");
  expect(document.activeElement?.getAttribute("aria-label")).toBe("composer.capability.removeUnavailable · second");
});

it("saves one fixed single-mode model only after confirming the context break", async () => {
  const catalog = settings();
  // nativeTarget is the only image-generation model, so single mode offers it beside "not selected".
  // nativeTarget 是唯一的图片生成模型，因此单模型模式把它与“未选择”并列提供。
  const nativeTarget = { ...catalog.providerToolTargets[0], target: { ...catalog.providerToolTargets[0].target,
    contract: { ...catalog.providerToolTargets[0].target.contract, capability: "image_generate" as const,
      extension_profile: { summary: "Generates images." } } } };
  catalog.providerToolTargets = [nativeTarget];
  // base enables nothing, so the fixed model is the only unsaved change.
  // base 不启用任何能力，使固定模型成为唯一未保存修改。
  const base = { ...session(), enabledCapabilities: [], mediaUnderstandingSelections: {} };
  vi.mocked(loadPublicAiSettings).mockResolvedValue(catalog);
  vi.mocked(loadPublicAiSessionProviderToolSelections).mockResolvedValue({ ...base, conflicts: [], requestsStarted: true });
  await render(<ComposerAiCapabilityControl ariaLabel="Capabilities" conversationId="c" />);
  await click("Capabilities");
  await selectCapabilityTab("settings.aiCapabilities.tab.generate");
  await act(async () => [...document.querySelectorAll<HTMLButtonElement>(".vk-cap-select")]
    .find((button) => button.textContent?.includes("settings.providerTools.imageGenerate"))!.click());
  const radios = () => [...document.querySelectorAll<HTMLInputElement>('.vk-cap-ext-list input[type="radio"]')];
  expect(radios().map((radio) => radio.checked)).toEqual([true, false]);
  await act(async () => radios()[1].click());
  expect(radios().map((radio) => radio.checked)).toEqual([false, true]);
  const extensionModes = { image_generate: { mode: "single" as const, single_target: { connection_id: "a", target_id: "image" } } };
  vi.mocked(savePublicAiSessionProviderToolSelections).mockResolvedValue({ ...base, extensionModes, conflicts: [], requestsStarted: true });
  const button = (label: string) => [...document.querySelectorAll<HTMLButtonElement>("button")]
    .find((candidate) => candidate.textContent === label);
  await act(async () => button("composer.capability.save")!.click());
  // A started conversation asks first; nothing is saved until the user accepts breaking the context.
  // 已开始的会话先询问；用户接受打破上下文之前不保存任何内容。
  expect(savePublicAiSessionProviderToolSelections).not.toHaveBeenCalled();
  await act(async () => button("composer.capability.contextBreak.confirm")!.click());
  expect(savePublicAiSessionProviderToolSelections).toHaveBeenCalledExactlyOnceWith({
    conversationId: "c", ...base, extensionModes,
  });
});

it("selects an existing media category with both modes when every backend is empty", async () => {
  const catalog = settings();
  catalog.providerToolTargets = [];
  await openCapabilities({ enabledCapabilities: [], selections: {}, mediaUnderstandingSelections: {}, extensionTargetSets: {}, extensionModes: {} }, catalog);
  expect(document.querySelectorAll('.vk-cap-select[aria-pressed="true"]')).toHaveLength(1);
  expect(document.querySelectorAll('[role="radio"]')).toHaveLength(2);
  expect(document.querySelector<HTMLButtonElement>('[role="combobox"][aria-label="settings.providerTools.model"]')!.disabled).toBe(true);
  expect(savePublicAiSessionProviderToolSelections).not.toHaveBeenCalled();
});
