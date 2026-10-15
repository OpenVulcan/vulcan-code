// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { KnowledgeBaseArea } from "./knowledge-base-area";
import { useKnowledgeRead } from "./use-knowledge-read";
import { knowledgeLibraryFixture } from "./knowledge-test-fixtures";

// Synthetic services preserve production controls without accessing real storage.
// 合成服务保留生产控件，不访问真实存储。
const reads = vi.hoisted(() => ({ libraries: vi.fn(), list: vi.fn(), create: vi.fn() }));
vi.mock("@/service/knowledge-base-service", async (original) => ({
  ...await original<typeof import("@/service/knowledge-base-service")>(),
  listKnowledgeLibraries: reads.libraries, listManagedKnowledgeDocuments: reads.list, createKnowledgeLibrary: reads.create,
}));
// Parameters: key is a stable catalog identity. Returns that identity for behavior assertions.
// 参数：key 为稳定目录身份。返回该身份以进行行为断言。
function translate(key: string): string { return key; }
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: translate, locale: "en-US" }) }));

// One root owns each test's effects and portal lifecycle.
// 一个根拥有每个测试的副作用及门户生命周期。
let root: Root;
// A detached host prevents interference with real app UI.
// 独立宿主防止干扰真实应用界面。
let host: HTMLDivElement;
beforeEach(() => {
  vi.resetAllMocks(); vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  reads.libraries.mockResolvedValue([knowledgeLibraryFixture]);
  reads.list.mockResolvedValue({ entries: [], nextCursor: null, headChangeSeq: 0 });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// Exposes one exact current read without adding mutation behavior.
// 暴露一个精确当前读取，不添加变更行为。
// Parameters: scope and load are the hook's actual inputs; expose captures its read-only refresh. Returns its current state.
// 参数：scope 及 load 为钩子真实输入；expose 捕获只读刷新。返回当前状态。
function ReadHarness({ scope, load, expose }: { scope: string; load: () => Promise<string>; expose: (refresh: (background?: boolean) => Promise<boolean>) => void }) {
  // The hook is the production ownership boundary under test.
// 此钩子为受测生产归属边界。
  const read = useKnowledgeRead(scope, load);
  React.useEffect(() => expose(read.refresh), [expose, read.refresh]);
  return <div data-loading={read.loading}>{read.value === null ? "no-value" : read.value}{read.error === null ? "" : read.error}</div>;
}

describe("knowledge reads and recovery", () => {
  it("distinguishes failed registry reads from empty results and retries reads alone", async () => {
    reads.libraries.mockRejectedValueOnce(new Error("registry unavailable"));
    await act(async () => root.render(<KnowledgeBaseArea projects={[]} />));
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("registry unavailable");
    expect(host.textContent).not.toContain("knowledge.empty");
    await act(async () => host.querySelector<HTMLButtonElement>('[role="alert"] button')!.click());
    expect(host.querySelector('[role="alert"]')).toBeNull();
    expect(host.textContent).toContain(knowledgeLibraryFixture.name);
    expect(reads.libraries).toHaveBeenCalledTimes(2);
    expect(reads.create).not.toHaveBeenCalled();
  });

  it("shows only deletion for an unreadable old format library", async () => {
    reads.libraries.mockResolvedValue([{ ...knowledgeLibraryFixture, lifecycleState: "error", settings: null, errorMessage: "Unsupported knowledge library format version 1" }]);
    await act(async () => root.render(<KnowledgeBaseArea projects={[]} />));
    await act(async () => host.querySelector<HTMLButtonElement>('[aria-pressed="false"]')!.click());
    expect(host.textContent).toContain("Unsupported knowledge library format version 1");
    expect(host.querySelector('[role="tablist"]')).toBeNull();
    expect(reads.list).not.toHaveBeenCalled();
    expect([...host.querySelectorAll("button")].some((button) => button.textContent === "knowledge.metadata")).toBe(false);
    expect([...host.querySelectorAll("button")].some((button) => button.textContent === "knowledge.delete")).toBe(true);
  });

  it("rejects older refresh results and clears a DTO after a failed confirmation read", async () => {
    // These deferred reads deliberately settle in the opposite order.
    // 这些延迟读取刻意按相反顺序完成。
    let finish!: (value: string) => void;
    // The first read remains pending while an explicit refresh becomes authoritative.
    // 首个读取保持等待，同时明确刷新成为权威。
    const load = vi.fn<() => Promise<string>>().mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; })).mockResolvedValueOnce("newest").mockRejectedValueOnce(new Error("confirmation failed")).mockResolvedValueOnce("recovered");
    // Capture the same public refresh used by all production recovery buttons.
    // 捕获全部生产恢复按钮使用的相同公开刷新。
    let refresh!: (background?: boolean) => Promise<boolean>;
    // Stable capture identity avoids changing the mounted read lifetime.
    // 稳定捕获身份避免改变挂载读取寿命。
    const expose = (value: typeof refresh): void => { refresh = value; };
    await act(async () => root.render(<ReadHarness scope="a" load={load} expose={expose} />));
    await act(async () => { await refresh(true); });
    await act(async () => finish("obsolete"));
    expect(host.textContent).toBe("newest");
    await act(async () => { expect(await refresh(true)).toBe(false); });
    expect(host.textContent).toContain("no-valueconfirmation failed");
    await act(async () => { await refresh(); });
    expect(host.textContent).toBe("recovered");
    expect(load).toHaveBeenCalledTimes(4);
  });

  it("revokes delayed callbacks when leaving and returning to the same scope", async () => {
    // The first A visit cannot publish into a later A visit.
    // 首次 A 访问不能发布到稍后 A 访问。
    let finish!: (value: string) => void;
    // A distinct loader remains pending only for its first lifetime.
    // 独立加载器仅首次寿命保持等待。
    const loadA = vi.fn<() => Promise<string>>().mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; })).mockResolvedValue("current-a");
    // B is an independent owner.
    // B 为独立所有者。
    const loadB = vi.fn<() => Promise<string>>().mockResolvedValue("current-b");
    // Exposed read-only callbacks are not used to reactivate obsolete lifetimes.
    // 暴露只读回调不用来重新激活过期寿命。
    const expose = vi.fn();
    await act(async () => root.render(<ReadHarness scope="a" load={loadA} expose={expose} />));
    // Retain the first visit's public callback to prove it cannot reactivate the later A visit.
    // 保留首次访问公开回调，以证明其不能重新激活稍后 A 访问。
    const oldRefresh = expose.mock.calls.at(-1)![0] as (background?: boolean) => Promise<boolean>;
    await act(async () => root.render(<ReadHarness scope="b" load={loadB} expose={expose} />));
    await act(async () => root.render(<ReadHarness scope="a" load={loadA} expose={expose} />));
    await act(async () => finish("obsolete-a"));
    await act(async () => { expect(await oldRefresh()).toBe(false); });
    expect(host.textContent).toBe("current-a");
    expect(loadA).toHaveBeenCalledTimes(2);
  });
});
