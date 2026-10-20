// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { loadOpenVulcanUsagePage, type OpenVulcanUsagePage } from "@/service/openvulcan-account-service";
import { OpenVulcanUsageHistory } from "./openvulcan-usage-history";

vi.mock("@/i18n", () => ({
  useI18n: () => ({ locale: "en-US", t: (key: string) => key }),
  formatMessage: (template: string) => template,
}));
vi.mock("@/service/openvulcan-account-service", () => ({ loadOpenVulcanUsagePage: vi.fn() }));

// host and root mount the actual history component with an isolated mocked host boundary.
// host 与 root 挂载真实历史组件，并隔离模拟宿主边界。
let host: HTMLDivElement;
let root: Root;
// firstPage deliberately exceeds JavaScript's exact integer range and carries an opaque cursor.
// firstPage 刻意超过 JavaScript 准确整数范围，并携带不透明游标。
const firstPage: OpenVulcanUsagePage = {
  from_date: "2026-10-01", to_date: "2026-10-12", next_cursor: "owner-bound+/cursor=",
  usage: [{ hour_start: "2026-10-12T00:00:00Z", model_id: "synthetic-vector", request_count: "9007199254740993", successful_requests: "9007199254740992", failed_requests: "1", input_tokens: "18014398509481987", charged_micro_points: "36028797018963977" }],
};

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.resetAllMocks();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

// cellTexts returns the rendered table cells' own text, excluding nested failure notes.
// cellTexts 返回表格单元格自身文本，不含嵌套的失败说明。
function cellTexts(): string[] {
  return Array.from(host.querySelectorAll("td"), (cell) => cell.firstChild?.textContent ?? "");
}

// Exact decimal strings are grouped without precision loss, and paging reuses the server's cursor unchanged.
// 准确十进制字符串分组时不丢精度，翻页原样使用服务端游标。
it("loads on mount, keeps large values exact and paginates with the owner-bound cursor", async () => {
  vi.mocked(loadOpenVulcanUsagePage).mockResolvedValueOnce(firstPage).mockResolvedValueOnce({ ...firstPage, usage: [], next_cursor: "" });
  await act(async () => root.render(<OpenVulcanUsageHistory />));
  expect(loadOpenVulcanUsagePage).toHaveBeenCalledExactlyOnceWith("");
  for (const exact of ["9,007,199,254,740,993", "18,014,398,509,481,987", "36,028,797,019.0"]) {
    expect(cellTexts()).toContain(exact);
  }
  const next = host.querySelector<HTMLButtonElement>(".openvulcan-usage-next")!;
  await act(async () => next.click());
  expect(loadOpenVulcanUsagePage).toHaveBeenLastCalledWith(firstPage.next_cursor);
  expect(host.querySelectorAll("td")).toHaveLength(0);
});

// Replacing the mounted account prevents an old in-flight page from appearing under the new account.
// 替换挂载账户可防止旧的在途页面出现在新账户下。
it("discards late usage after the account connection changes", async () => {
  // resolvers control each mount's outstanding request in call order.
  // resolvers 按调用顺序控制每次挂载尚未完成的请求。
  const resolvers: Array<(page: OpenVulcanUsagePage) => void> = [];
  vi.mocked(loadOpenVulcanUsagePage).mockImplementation(() => new Promise((resolve) => { resolvers.push(resolve); }));
  await act(async () => root.render(<OpenVulcanUsageHistory key="account-a" />));
  await act(async () => root.render(<OpenVulcanUsageHistory key="account-b" />));
  expect(resolvers).toHaveLength(2);
  await act(async () => resolvers[0](firstPage));
  expect(host.querySelectorAll("td")).toHaveLength(0);
  await act(async () => resolvers[1]({ ...firstPage, next_cursor: "" }));
  expect(host.querySelectorAll("tbody tr")).toHaveLength(1);
});
