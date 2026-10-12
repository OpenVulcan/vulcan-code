// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import OpenVulcanAccountDialog from "./openvulcan-account-dialog";
import { OpenVulcanAccountContext, type OpenVulcanAccountController } from "./openvulcan-account-context";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));

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
  expect(button("openvulcan.account.china").disabled).toBe(true);
  expect(button("openvulcan.account.global").disabled).toBe(true);
  await act(async () => button("openvulcan.account.skip").click());
  expect(account.setOpen).toHaveBeenCalledExactlyOnceWith(false);
});
