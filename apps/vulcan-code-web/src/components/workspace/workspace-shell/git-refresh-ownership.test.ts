import { expect, it } from "vitest";
import { createWorkspaceGitRefreshOwnership } from "./git-refresh-ownership";

it("does not let an older rejection overwrite a newer successful refresh", async () => {
  const owner = createWorkspaceGitRefreshOwnership();
  const old = owner.begin("X:/audit-a");
  let rejectOld!: (reason: Error) => void;
  let displayed = "loading";
  // delayed models the real catalog failure boundary with generation ownership checked on completion.
  // delayed 模拟真实目录失败边界，在完成时检查请求代次所有权。
  const delayed = new Promise<void>((_resolve, reject) => { rejectOld = reject; }).catch(error => {
    if (owner.owns(old)) displayed = error.message;
  });
  const latest = owner.begin("X:/audit-a");
  if (owner.owns(latest)) displayed = "new catalog";
  rejectOld(new Error("old error")); await delayed;
  expect(displayed).toBe("new catalog");
  expect(owner.ownsSelection(old, "X:/audit-a")).toBe(false);
});

it("permits independent project caches without changing the currently selected project", () => {
  const owner = createWorkspaceGitRefreshOwnership();
  const a = owner.begin("X:/audit-a");
  const b = owner.begin("X:/audit-b");
  expect(owner.owns(a)).toBe(true);
  expect(owner.owns(b)).toBe(true);
  expect(owner.ownsSelection(a, "X:/audit-b")).toBe(false);
  expect(owner.ownsSelection(b, "X:/audit-b")).toBe(true);
  expect(owner.ownsSelection(b, null)).toBe(false);
});
