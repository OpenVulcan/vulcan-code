// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { loadOpenVulcanUsagePage, type OpenVulcanUsagePage } from "@/service/openvulcan-account-service";
import { OpenVulcanUsageHistory } from "./openvulcan-usage-history";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
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

// Exact decimal strings survive rendering and the next request uses the server's cursor unchanged.
// 准确十进制字符串在渲染中保持不变，后续请求原样使用服务端游标。
it("preserves large usage values and paginates with the owner-bound cursor", async () => {
  vi.mocked(loadOpenVulcanUsagePage).mockResolvedValueOnce(firstPage).mockResolvedValueOnce({ ...firstPage, usage: [], next_cursor: "" });
  await act(async () => root.render(<OpenVulcanUsageHistory />));
  expect(loadOpenVulcanUsagePage).not.toHaveBeenCalled();
  await act(async () => host.querySelector<HTMLButtonElement>("button")!.click());
  expect(loadOpenVulcanUsagePage).toHaveBeenCalledWith("");
  for (const exact of [firstPage.usage[0].request_count, firstPage.usage[0].input_tokens, firstPage.usage[0].charged_micro_points]) {
    expect(Array.from(host.querySelectorAll("td"), (cell) => cell.textContent)).toContain(exact);
  }
  // next is located by its stable catalog key rather than translated copy or DOM position.
  // next 按稳定目录键定位，不依赖翻译文案或 DOM 位置。
  const next = Array.from(host.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent === "openvulcan.usage.next")!;
  await act(async () => next.click());
  expect(loadOpenVulcanUsagePage).toHaveBeenLastCalledWith(firstPage.next_cursor);
  expect(host.querySelectorAll("td")).toHaveLength(0);
});

// Replacing the mounted account prevents an old in-flight page from appearing under the new account.
// 替换挂载账户可防止旧的在途页面出现在新账户下。
it("discards late usage after the account connection changes", async () => {
  // resolvePage controls only the old account's outstanding request.
  // resolvePage 仅控制旧账户尚未完成的请求。
  let resolvePage!: (page: OpenVulcanUsagePage) => void;
  vi.mocked(loadOpenVulcanUsagePage).mockReturnValue(new Promise((resolve) => { resolvePage = resolve; }));
  await act(async () => root.render(<OpenVulcanUsageHistory key="account-a" />));
  await act(async () => host.querySelector<HTMLButtonElement>("button")!.click());
  await act(async () => root.render(<OpenVulcanUsageHistory key="account-b" />));
  await act(async () => resolvePage(firstPage));
  expect(host.querySelectorAll("td")).toHaveLength(0);
  expect(loadOpenVulcanUsagePage).toHaveBeenCalledTimes(1);
});
