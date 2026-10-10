// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PermissionJudgeRecord, PermissionJudgeRecordPage } from "@/types";

// The translator returns keys, so assertions name controls by catalog key instead of wording.
// 翻译函数返回键名，使断言以目录键而非文案定位控件。
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: string) => key;
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});

// Every service call is synthetic; nothing reaches a running service.
// 所有服务调用均为合成；不会访问运行中的服务。
const service = vi.hoisted(() => ({
  loadPermissionJudgeRecords: vi.fn(),
  markPermissionJudgeRecord: vi.fn(),
  exportPermissionJudgeRecords: vi.fn(),
  clearPermissionJudgeRecords: vi.fn(),
}));
vi.mock("@/service/permission-judge-service", () => service);

const { PermissionJudgeRecordsPage } = await import("./permission-judge-records-page");

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

// record builds a judged record; without a label the judge failed and kept no trace.
// record 构建一条已判定记录；没有标签时表示判定失败且没有轨迹。
function record(id: string, label: "allow" | "block" | null): PermissionJudgeRecord {
  return {
    id, judgedAtMs: 1_700_000_000_000, sessionId: "s", workspaceRoot: "/w/a", toolName: "shell_command",
    arguments: `{"command":"cargo build ${id}"}`, highRisk: null,
    judge: {
      label, error: label ? null : "worker stopped", elapsedMs: 4.2,
      trace: label ? {
        backend: "local", model: "m", policyVersion: "1", settings: {}, rendered: "Tool: shell_command",
        logits: [1, 0, -1], allowProbability: null, inputTokens: 12,
      } : null,
    },
    review: null, outcome: label === "allow" ? "judge_allowed" : "user_denied", correction: null,
  };
}

function page(records: PermissionJudgeRecord[], extra: Partial<PermissionJudgeRecordPage> = {}): PermissionJudgeRecordPage {
  return {
    records, hasMore: false, total: records.length, marked: 0, workspaces: ["/w/a"],
    retentionRecords: 5000, retentionDays: 30, ...extra,
  };
}

async function mount(first: PermissionJudgeRecordPage) {
  service.loadPermissionJudgeRecords.mockResolvedValue(first);
  await act(async () => root.render(<PermissionJudgeRecordsPage />));
}

// A filter reloads the first page with the chosen judge result.
// 筛选会以所选判定结果重新读取第一页。
it("reloads the first page when a judge result filter is chosen", async () => {
  await mount(page([record("a", "allow")]));
  expect(service.loadPermissionJudgeRecords).toHaveBeenLastCalledWith(
    { label: null, workspace: null, markedOnly: false, before: null, limit: 50 });
  await act(async () => host.querySelector<HTMLButtonElement>('[data-records-filter="block"]')!.click());
  expect(service.loadPermissionJudgeRecords).toHaveBeenLastCalledWith(
    { label: "block", workspace: null, markedOnly: false, before: null, limit: 50 });
});

// Loading more continues after the last shown record and appends the next page.
// 加载更多会从最后显示的记录之后继续，并追加下一页。
it("continues after the last shown record", async () => {
  await mount(page([record("a", "allow")], { hasMore: true, total: 2 }));
  service.loadPermissionJudgeRecords.mockResolvedValue(page([record("b", "allow")], { total: 2 }));
  await act(async () => host.querySelector<HTMLButtonElement>("[data-records-more]")!.click());
  expect(service.loadPermissionJudgeRecords).toHaveBeenLastCalledWith(
    expect.objectContaining({ before: "a" }));
  expect(Array.from(host.querySelectorAll("[data-record-id]"), (item) => item.getAttribute("data-record-id")))
    .toEqual(["a", "b"]);
  expect(host.querySelector("[data-records-more]")).toBeNull();
});

// Marks are offered only for records the judge scored, and a mark updates the record in place.
// 只有判定器评过分的记录才可标记，标记后原地更新该记录。
it("marks only scored records and shows the saved mark", async () => {
  await mount(page([record("a", "allow"), record("b", null)]));
  const marks = (id: string) => host.querySelector(`[data-record-id="${id}"] [data-record-mark]`);
  expect(marks("a")).not.toBeNull();
  expect(marks("b")).toBeNull();
  service.markPermissionJudgeRecord.mockResolvedValue({ ...record("a", "allow"), correction: "block" });
  const block = Array.from(marks("a")!.querySelectorAll("button"))
    .find((button) => button.textContent === "settings.permissionJudge.records.label.block")!;
  await act(async () => block.click());
  expect(service.markPermissionJudgeRecord).toHaveBeenCalledExactlyOnceWith("a", "block");
  expect(host.querySelector('[data-record-id="a"] [data-record-correction="block"]')).not.toBeNull();
  expect(host.querySelector<HTMLButtonElement>("[data-records-export]")!.disabled).toBe(false);
});

// Clearing asks first and reloads the empty log after confirmation.
// 清除会先确认，确认后重新读取已清空的记录。
it("clears only after confirmation", async () => {
  await mount(page([record("a", "allow")]));
  expect(host.querySelector<HTMLButtonElement>("[data-records-export]")!.disabled).toBe(true);
  await act(async () => host.querySelector<HTMLButtonElement>("[data-records-clear]")!.click());
  expect(service.clearPermissionJudgeRecords).not.toHaveBeenCalled();
  service.clearPermissionJudgeRecords.mockResolvedValue(undefined);
  service.loadPermissionJudgeRecords.mockResolvedValue(page([]));
  await act(async () => document.querySelector<HTMLButtonElement>("[data-records-clear-confirm]")!.click());
  expect(service.clearPermissionJudgeRecords).toHaveBeenCalledOnce();
  expect(host.querySelector("[data-record-id]")).toBeNull();
});
