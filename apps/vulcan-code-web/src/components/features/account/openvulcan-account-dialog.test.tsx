// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { loadOpenVulcanUsagePage } from "@/service/openvulcan-account-service";
import { queryPublicAiConnectionUsage } from "@/service/settings-actions-service";
import OpenVulcanAccountDialog from "./openvulcan-account-dialog";
import { OpenVulcanAccountContext, type OpenVulcanAccountController } from "./openvulcan-account-context";

vi.mock("@/i18n", () => ({
  useI18n: () => ({ locale: "en-US", t: (key: string) => key }),
  formatMessage: (template: string) => template,
}));
vi.mock("@/service/settings-actions-service", () => ({ queryPublicAiConnectionUsage: vi.fn() }));
vi.mock("@/service/openvulcan-account-service", () => ({ loadOpenVulcanUsagePage: vi.fn() }));

// host and root mount the real portal and disclosure, independently of the settings page.
// host 与 root 挂载真实门户与展开组件，独立于设置页。
let host: HTMLDivElement;
let root: Root;
// account supplies only public state and mocked recovery operations.
// account 仅提供公开状态及模拟恢复操作。
let account: OpenVulcanAccountController;

// render mounts the current public account observation; returns after React commits the dialog.
// render 挂载当前公开账户观测；在 React 提交弹窗后返回。
async function render() {
  await act(async () => root.render(<OpenVulcanAccountContext.Provider value={account}><OpenVulcanAccountDialog /></OpenVulcanAccountContext.Provider>));
}

// button locates a semantic action by its stable catalog key, independent of language or DOM order.
// button 按稳定目录键定位语义操作，不依赖语言或 DOM 顺序。
// Returns the matching button or throws a useful stale-contract diagnostic.
// 返回匹配按钮，或抛出可识别契约过期的诊断。
function button(key: string): HTMLButtonElement {
  const match = Array.from(document.querySelectorAll<HTMLButtonElement>("button")).find((candidate) => candidate.textContent?.includes(key));
  if (!match) throw new Error(`Expected catalog action may have changed: ${key}`);
  return match;
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  account = {
    snapshot: null, initialized: true, open: true, busy: false, waiting: false,
    error: { stage: "connect", detail: `Synthetic account failure: https://example.invalid/${"long-segment/".repeat(160)}`, region: "china" },
    setOpen: vi.fn(), login: vi.fn(), logout: vi.fn(), refresh: vi.fn(),
  };
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

// Failed connection details stay folded until requested, remain copyable in full, and retry the chosen region.
// 连接失败详情在主动请求前保持收起，可完整复制，且重试所选区域。
it("folds technical details and retries the exact regional login", async () => {
  await render();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("openvulcan.account.error.connect.title");
  expect(button("settings.errors.showErrorDetails").getAttribute("aria-expanded")).toBe("false");
  await act(async () => button("settings.errors.showErrorDetails").click());
  expect(button("settings.errors.hideErrorDetails").getAttribute("aria-expanded")).toBe("true");
  expect(document.querySelector("pre")?.textContent).toBe(account.error!.detail);
  // writeText observes exactly what a user can copy, including a long unbroken diagnostic.
  // writeText 观察用户可复制的准确内容，包括很长的连续诊断。
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  await act(async () => button("settings.errors.copyError").click());
  expect(writeText).toHaveBeenCalledWith(account.error!.detail);
  await act(async () => button("openvulcan.account.retryLogin").click());
  expect(account.login).toHaveBeenCalledExactlyOnceWith("china");
});

// A new failed operation starts collapsed, and observation errors recover by reading rather than signing in.
// 新失败操作从收起状态开始，且观测错误通过读取恢复，而非重新登录。
it("resets disclosure for a new error and refreshes uncertain account state", async () => {
  await render();
  await act(async () => button("settings.errors.showErrorDetails").click());
  account = { ...account, error: { stage: "load", detail: "Synthetic observation error", region: "global" } };
  await render();
  expect(button("settings.errors.showErrorDetails").getAttribute("aria-expanded")).toBe("false");
  await act(async () => button("openvulcan.account.refresh").click());
  expect(account.refresh).toHaveBeenCalledTimes(1);
  expect(account.login).not.toHaveBeenCalled();
});

// An active retry hides stale failure guidance and prevents competing regional authorizations.
// 正在重试时隐藏过期失败说明，并阻止并发区域授权。
it("shows pending progress and allows cancelling the waiting browser flow", async () => {
  account = { ...account, waiting: true };
  await render();
  expect(document.querySelector('[role="alert"]')).toBeNull();
  expect(document.querySelector('[role="status"]')).not.toBeNull();
  for (const site of document.querySelectorAll<HTMLButtonElement>("[data-openvulcan-site]")) expect(site.disabled).toBe(true);
  expect(document.querySelector<HTMLButtonElement>("[data-openvulcan-login]")!.disabled).toBe(true);
  await act(async () => button("openvulcan.account.skip").click());
  expect(account.setOpen).toHaveBeenCalledExactlyOnceWith(false);
});

// Signing in uses the chosen site, which defaults from the interface language and can be switched.
// 登录使用所选站点；站点按界面语言默认，并可切换。
it("signs in to the selected site", async () => {
  account = { ...account, error: null };
  await render();
  expect(document.querySelector('[data-openvulcan-site="global"]')!.getAttribute("aria-checked")).toBe("true");
  await act(async () => document.querySelector<HTMLButtonElement>('[data-openvulcan-site="china"]')!.click());
  await act(async () => document.querySelector<HTMLButtonElement>("[data-openvulcan-login]")!.click());
  expect(account.login).toHaveBeenCalledExactlyOnceWith("china");
});

// A signed-in account shows its name initial and sums its exact quota lots for the plan balance.
// 已登录账户显示名称首字符，并精确累加额度批次作为套餐余额。
it("shows the account initial and the exact remaining plan balance", async () => {
  vi.mocked(loadOpenVulcanUsagePage).mockResolvedValue({ from_date: "2026-10-01", to_date: "2026-10-12", next_cursor: "", usage: [] });
  vi.mocked(queryPublicAiConnectionUsage).mockResolvedValue({
    credential_id: "", availability: "supported", plan_name: "plus",
    quota_windows: ["9007199254740.000001", "0.500000"].map((remaining, index) => ({
      id: `lot-${index}`, display_name: "member_window", bucket_name: "member_window",
      remaining, total: "9007199254741.000000", unit: "points", unlimited: false, available: true,
    })),
  });
  account = { ...account, error: null, snapshot: { connected: true, connectionId: "synthetic-connection", displayName: "lingeasy", region: "china", message: null } };
  await render();
  expect(queryPublicAiConnectionUsage).toHaveBeenCalledExactlyOnceWith("synthetic-connection");
  expect(document.querySelector(".openvulcan-account-avatar")?.textContent).toBe("L");
  expect(document.querySelector(".openvulcan-plan-tier")?.textContent).toBe("openvulcan.plan.tier.plus");
  expect(document.querySelector(".openvulcan-plan-balance strong")?.textContent).toBe("9,007,199,254,740.500001");
});
