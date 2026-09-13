import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../src/i18n";
import { VmmNativeSessionBrowser, nativeSessionInitialQuery } from "../../src/components/features/memory/vmm-native-session-browser";
import type { VmmMemoryActionResult } from "../../src/types";
import { workbench, nativeCatalog } from "./fixtures";
import { memoryRead } from "./memory-fixtures";
import { memorySessionRead } from "./memory-session-fixtures";

// Select the nonempty isolated scenario before static fixture modules read the page URL.
// 在静态夹具模块读取页面网址之前选择非空隔离场景。
vi.hoisted(() => { window.history.replaceState({}, "", "?scenario=invalid-default"); Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); });

// response bridges only admitted fixture reads into the real action envelope.
// response 仅将准入夹具读取桥接到真实动作信封。
// Parameters: action/payload preserve original values. Returns: a typed result or an explicit unsupported error.
// 参数：action/payload 保留原始值。返回值：强类型结果或显式不支持错误。
async function response(action: string, payload: Record<string, unknown> = {}): Promise<VmmMemoryActionResult> {
  // data must come from the same handler as the isolated browser runtime.
  // data 必须来自与隔离浏览器运行时相同的处理器。
  const data = memoryRead(action, payload);
  if (!data || data.kind === "empty") throw new Error(`Unsupported fixture read: ${action}`);
  return { action, kind: action, summary: "隔离只读夹具", request: payload, response: data.value, data };
}

describe("native memory read-only provenance fixtures", () => {
  // Verifies pagination, filter identity and exact turn-to-child relationships without any mutation handlers.
  // 验证分页、筛选身份及准确回合到子记忆关系，不提供任何变更处理器。
  it("keeps summary pages lightweight and detail reads bound to the selected session", () => {
    // query uses the actual production initial-query builder.
    // query 使用实际生产初始查询构造器。
    const query = nativeSessionInitialQuery(nativeCatalog.projects[0].projectId);
    const page = memorySessionRead("native_session_list", { ...query, limit: 1 });
    expect(page?.kind).toBe("native_session_list");
    if (page?.kind !== "native_session_list") throw new Error("missing page");
    expect(page.value.total).toBe(2);
    expect(page.value.sessions).toHaveLength(1);
    expect(page.value).not.toHaveProperty("turns");
    const detail = memorySessionRead("native_memory_records", { vmmProjectId: query.vmmProjectId, sessionSourceId: page.value.sessions[0].sessionSourceId });
    if (detail?.kind !== "native_memory_records") throw new Error("missing detail");
    expect(detail.value.turns).toHaveLength(2);
    expect(detail.value.memories).toHaveLength(2);
    expect(detail.value.turns[0].generatedMemoryIds).toEqual(detail.value.memories.map((memory) => memory.memoryId));
    expect(detail.value.memories.every((memory) => memory.sourceTurnId === detail.value.turns[0].turnId)).toBe(true);
    const archived = memorySessionRead("native_session_list", { ...query, status: "archived" });
    expect(archived?.kind === "native_session_list" && archived.value.sessions.every((session) => session.status === "archived")).toBe(true);
    expect(() => memorySessionRead("native_session_list", { ...query, expectedStorageVersion: 1 })).toThrow("STALE_MEMORY_VERSION");
    expect(() => memorySessionRead("native_memory_records", { vmmProjectId: query.vmmProjectId, sessionSourceId: "unknown" })).toThrow("UNKNOWN_MEMORY_SESSION");
    expect(memorySessionRead("native_memory_write", {})).toBeUndefined();
  });

  // Exercises the actual list, selected detail, turn sheet and child-memory sheet against the same fixture reader.
  // 针对同一夹具读取器操作真实列表、选中详情、回合侧栏及子记忆侧栏。
  it("opens a session, extracted turn and child memory through actual controls", async () => {
    // snapshot is isolated workbench data and explicitly selects its first authoritative project.
    // snapshot 是隔离工作台数据，并明确选择其第一个权威项目。
    const snapshot = structuredClone(workbench.vmmMemory);
    snapshot.selectedProjectId = nativeCatalog.bindings[0].vulcanProjectId;
    const onRunAction = vi.fn(response);
    const container = document.createElement("div");
    document.body.append(container);
    Object.defineProperties(container, { scrollHeight: { value: 1200 }, clientHeight: { value: 500 } });
    container.scrollTop = 73;
    const root = createRoot(container);
    // click locates a specific visible user-facing action in its actual rendered surface.
    // click 在实际渲染表面定位指定用户动作。
    const click = (scope: ParentNode, label: string) => {
      const button = [...scope.querySelectorAll<HTMLButtonElement>("button")].find((entry) => entry.textContent?.includes(label));
      if (!button) throw new Error(`Missing rendered button: ${label}`);
      button.click();
    };
    try {
      await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeSessionBrowser snapshot={snapshot} busy={false} onRunAction={onRunAction} onCheckUpdates={(query) => response("native_session_list", { ...query })} /></I18nProvider>));
      expect(onRunAction.mock.calls.some(([action]) => action === "native_memory_records")).toBe(false);
      // retainedList must remain mounted without occupying the selected detail surface.
      // retainedList 必须保持挂载，但不得占据选中详情表面。
      const retainedList = container.querySelector<HTMLElement>(".vmm-session-browser")!;
      const searchDraft = retainedList.querySelector<HTMLInputElement>('input[name="search"]')!;
      searchDraft.value = "尚未应用的筛选草稿";
      await act(async () => click(container, "隔离验收：数据库设计与提炼"));
      expect(retainedList.hidden).toBe(true);
      expect(container.scrollTop).toBe(0);
      expect(container.textContent).toContain("隔离样例：数据库采用 PostgreSQL");
      await act(async () => click(container.querySelector(".vmm-session-detail")!, "隔离样例：数据库采用 PostgreSQL"));
      expect(document.querySelector('[role="dialog"]')?.textContent).toContain("合成提炼分析");
      await act(async () => click(document.querySelector('[role="dialog"]')!, "隔离子记忆：数据库选型"));
      expect(document.querySelector('[role="dialog"]')?.textContent).toContain("用于验证完整正文、来源回合与返回导航");
      await act(async () => click(document.querySelector('[role="dialog"]')!, `#${nativeCatalog.projects[0].projectId * 1000 + 1}`));
      expect(document.querySelector('[role="dialog"]')?.textContent).toContain("合成提炼分析");
      await act(async () => document.querySelector<HTMLButtonElement>('[role="dialog"] [data-slot="sheet-close"]')!.click());
      await act(async () => click(container.querySelector(".vmm-session-detail")!, "返回会话列表"));
      expect(retainedList.hidden).toBe(false);
      expect(container.querySelector(".vmm-session-browser")).toBe(retainedList);
      expect(searchDraft.value).toBe("尚未应用的筛选草稿");
      expect(container.scrollTop).toBe(73);
      expect(onRunAction.mock.calls.every(([action]) => ["native_project_catalog", "native_session_list", "native_memory_records"].includes(action))).toBe(true);
    } finally {
      await act(async () => root.unmount());
      container.remove();
    }
  });
});
