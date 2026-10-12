// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import * as service from "@/service/openvulcan-account-service";
import { useOpenVulcanAccount } from "./use-openvulcan-account";
import type { PublicAiOAuthTransaction } from "@/types";

vi.mock("@/service/webui-service-runtime", () => ({ hasServiceRuntime: () => true }));
vi.mock("@/service/openvulcan-account-service", () => ({ loadOpenVulcanAccount: vi.fn(), claimOpenVulcanPrompt: vi.fn(), startOpenVulcanLogin: vi.fn(), openVulcanLoginStatus: vi.fn(), cancelOpenVulcanLogin: vi.fn(), commitOpenVulcanLogin: vi.fn(), logoutOpenVulcanAccount: vi.fn() }));

// host and root mount the real lifecycle hook; epoch separates simulated host generations.
// host 与 root 挂载真实生命周期钩子；epoch 隔离模拟宿主代际。
let host: HTMLDivElement;
let root: Root;
let epoch = 0;
let account: ReturnType<typeof useOpenVulcanAccount>;
// signedOut and signedIn are public observations without any browser-side token.
// signedOut 与 signedIn 是不包含浏览器侧令牌的公开观测。
const signedOut: service.OpenVulcanAccountSnapshot = { connected: false, connectionId: null, displayName: null, region: null, message: null };
const signedIn: service.OpenVulcanAccountSnapshot = { connected: true, connectionId: "account-a", displayName: "alice", region: "china", message: null };
// transaction identifies the single server-owned callback flow used by these tests.
// transaction 标识这些测试使用的唯一服务端回调流程。
const transaction: PublicAiOAuthTransaction = { transactionId: "login-a", authorizationBinding: "regional-account", phase: "waiting_for_callback", expiresAtUnixMs: 2_000_000_000_000 };

// Harness exposes the actual account controller for one host epoch and startup readiness.
// Harness 暴露一个宿主代际与启动就绪状态的真实账户控制器。
function Harness(props: { ready?: boolean }) { account = useOpenVulcanAccount(props.ready !== false, epoch); return null; }

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.resetAllMocks();
  vi.useFakeTimers();
  epoch += 1;
  vi.mocked(service.loadOpenVulcanAccount).mockResolvedValue(signedOut);
  vi.mocked(service.claimOpenVulcanPrompt).mockResolvedValue(true);
  vi.mocked(service.startOpenVulcanLogin).mockResolvedValue({ transaction, authorizationUrl: "https://auth.openvulcan.cn/authorize", flowKind: "loopback_callback" });
  vi.mocked(service.openVulcanLoginStatus).mockResolvedValue(transaction);
  vi.mocked(service.cancelOpenVulcanLogin).mockResolvedValue();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

// StrictMode must neither lose the first prompt nor repeat it after skipping and remounting.
// StrictMode 既不能丢失首次提示，也不能在跳过并重挂载后重复展示。
it("claims once after readiness and preserves skip across StrictMode remounts", async () => {
  await act(async () => root.render(<React.StrictMode><Harness ready={false} /></React.StrictMode>));
  expect(service.claimOpenVulcanPrompt).not.toHaveBeenCalled();
  await act(async () => root.render(<React.StrictMode><Harness /></React.StrictMode>));
  expect(account.open).toBe(true);
  expect(service.claimOpenVulcanPrompt).toHaveBeenCalledTimes(1);
  await act(async () => account.setOpen(false));
  await act(async () => root.unmount());
  root = createRoot(host);
  vi.mocked(service.loadOpenVulcanAccount).mockResolvedValue(signedIn);
  await act(async () => root.render(<Harness />));
  expect(account.open).toBe(false);
  expect(account.snapshot?.connectionId).toBe(signedIn.connectionId);
  expect(service.claimOpenVulcanPrompt).toHaveBeenCalledTimes(1);
});

// Successful authorization is committed once and makes the shared connection visible.
// 成功授权仅提交一次，并使共享连接可见。
it("commits the selected regional login once", async () => {
  await act(async () => root.render(<Harness />));
  vi.mocked(service.openVulcanLoginStatus).mockResolvedValue({ ...transaction, phase: "succeeded" });
  vi.mocked(service.loadOpenVulcanAccount).mockResolvedValue(signedIn);
  await act(async () => account.login("china"));
  expect(service.startOpenVulcanLogin).toHaveBeenCalledWith("china");
  expect(service.commitOpenVulcanLogin).toHaveBeenCalledExactlyOnceWith(transaction.transactionId);
  expect(account.snapshot?.connected).toBe(true);
  expect(account.open).toBe(false);
  expect(account.busy).toBe(false);
});

// Closing a pending prompt invalidates an already-running status observation before it can commit.
// 关闭待处理提示会在提交前使已经发出的状态观测失效。
it("rejects a late successful callback after the user skips", async () => {
  // resolveStatus releases a response only after cancellation has completed locally.
  // resolveStatus 仅在本地取消完成后释放响应。
  let resolveStatus!: (value: PublicAiOAuthTransaction) => void;
  vi.mocked(service.openVulcanLoginStatus).mockReturnValue(new Promise((resolve) => { resolveStatus = resolve; }));
  await act(async () => root.render(<Harness />));
  await act(async () => account.login("global"));
  await act(async () => account.setOpen(false));
  await act(async () => resolveStatus({ ...transaction, phase: "succeeded" }));
  expect(service.cancelOpenVulcanLogin).toHaveBeenCalledWith(transaction.transactionId);
  expect(service.commitOpenVulcanLogin).not.toHaveBeenCalled();
  expect(account.snapshot?.connected).toBe(false);
});

// Local sign-out is authoritative even when the remote revocation request fails offline.
// 即使离线时远程撤销请求失败，本地退出仍是权威状态。
it("reconciles local logout after a remote revocation error", async () => {
  vi.mocked(service.loadOpenVulcanAccount).mockResolvedValue(signedIn);
  await act(async () => root.render(<Harness />));
  vi.mocked(service.logoutOpenVulcanAccount).mockRejectedValue(new Error("remote unavailable"));
  vi.mocked(service.loadOpenVulcanAccount).mockResolvedValue(signedOut);
  await act(async () => account.logout());
  expect(account.snapshot?.connectionId).toBeNull();
  expect(account.error).toBeTruthy();
  expect(account.busy).toBe(false);
});

// Waiting browser flows suspend background account reads, preventing stale pre-commit state from winning.
// 等待浏览器的流程暂停后台账户读取，防止提交前的过期状态覆盖新状态。
it("does not refresh account state across a pending login", async () => {
  await act(async () => root.render(<Harness />));
  await act(async () => account.login("china"));
  vi.mocked(service.loadOpenVulcanAccount).mockClear();
  await act(async () => vi.advanceTimersByTimeAsync(60_000));
  expect(service.loadOpenVulcanAccount).not.toHaveBeenCalled();
});

// A failed initial host read leaves the first-run claim available to a subsequent successful mount.
// 首次宿主读取失败后，后续成功挂载仍可获得首启认领。
it("retries an unclaimed bootstrap after a transient host failure", async () => {
  vi.mocked(service.loadOpenVulcanAccount).mockRejectedValueOnce(new Error("host restarting"));
  await act(async () => root.render(<Harness />));
  expect(service.claimOpenVulcanPrompt).not.toHaveBeenCalled();
  await act(async () => root.unmount());
  root = createRoot(host);
  await act(async () => root.render(<Harness />));
  expect(service.claimOpenVulcanPrompt).toHaveBeenCalledTimes(1);
  expect(account.open).toBe(true);
});

// A post-authorization connection failure retains the chosen region and starts a fresh transaction on retry.
// 授权后的连接失败保留所选区域，并在重试时发起新事务。
it("reports a connection failure separately and allows a fresh regional login", async () => {
  await act(async () => root.render(<Harness />));
  vi.mocked(service.openVulcanLoginStatus).mockResolvedValue({ ...transaction, phase: "succeeded" });
  vi.mocked(service.commitOpenVulcanLogin).mockRejectedValue(new Error("synthetic account endpoint failure"));
  await act(async () => account.login("china"));
  expect(account.error).toMatchObject({ stage: "connect", region: "china" });
  expect(account.open).toBe(true);
  expect(account.waiting).toBe(false);
  expect(account.busy).toBe(false);
  vi.mocked(service.openVulcanLoginStatus).mockResolvedValue(transaction);
  await act(async () => account.login(account.error!.region!));
  expect(service.startOpenVulcanLogin).toHaveBeenNthCalledWith(2, "china");
  expect(account.error).toBeNull();
  expect(account.waiting).toBe(true);
});

// Retrying a failed observation reads the saved account instead of replaying its already-completed authorization.
// 重试失败观测时读取已保存账户，不重放已完成授权。
it("refreshes an account after commit without authorizing or committing twice", async () => {
  await act(async () => root.render(<Harness />));
  vi.mocked(service.openVulcanLoginStatus).mockResolvedValue({ ...transaction, phase: "succeeded" });
  vi.mocked(service.loadOpenVulcanAccount).mockRejectedValue(new Error("synthetic observation failure"));
  await act(async () => account.login("global"));
  expect(account.error?.stage).toBe("load");
  vi.mocked(service.loadOpenVulcanAccount).mockResolvedValue(signedIn);
  await act(async () => account.refresh());
  expect(account.snapshot?.connected).toBe(true);
  expect(account.error).toBeNull();
  expect(service.startOpenVulcanLogin).toHaveBeenCalledTimes(1);
  expect(service.commitOpenVulcanLogin).toHaveBeenCalledTimes(1);
});
