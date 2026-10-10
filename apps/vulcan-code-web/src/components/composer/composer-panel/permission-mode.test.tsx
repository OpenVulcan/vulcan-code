// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { toast } from "sonner";
import type { PermissionMode } from "@/service/permission-mode";
import { setSessionPermissionMode, type SessionPermissionModeUpdateResponse } from "@/service/permission-mode-service";
import { useComposerSessionSettings } from "./use-composer-answer-mode";
import type { PermissionModeOwner } from "./use-composer-permission-mode";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), info: vi.fn() } }));
vi.mock("@/service/permission-mode-service", () => ({ setSessionPermissionMode: vi.fn() }));

// root/host isolate actual hook lifetimes; only RPC and text catalogs are synthetic.
// root/host 隔离真实钩子生命周期；仅 RPC 与文案目录使用合成边界。
let root: Root;
let host: HTMLDivElement;
let settings: ReturnType<typeof useComposerSessionSettings>;
const save = vi.mocked(setSessionPermissionMode);

/** Render the actual session settings hooks for one exact permission owner.
 * 为一个精确权限归属渲染真实会话设置钩子。 */
function Harness({ sessionId = null, authority = null, slot = "draft:a" }: {
  sessionId?: string | null; authority?: PermissionModeOwner | null; slot?: string;
}) {
  settings = useComposerSessionSettings(slot, sessionId, null, "standard", authority);
  return null;
}

/** Build a persisted owner from a synthetic session and mode.
 * 从合成会话与模式构建持久归属。 */
function authority(mode: PermissionMode = "workspace_write", sessionId = "session-a"): PermissionModeOwner {
  return { sessionId, generation: 1, mode };
}

/** Build a durable service receipt for one session.
 * 为一个会话构建可靠服务回执。 */
function receipt(mode: PermissionMode, sessionId = "session-a"): SessionPermissionModeUpdateResponse {
  return { session_id: sessionId, revision: 2, permission_mode: mode };
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.clearAllMocks();
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it("shows the session default for drafts and captures only explicit choices", async () => {
  await act(async () => root.render(<Harness />));
  expect(settings.permissionMode.mode).toBe("workspace_write");
  expect(settings.draftSessionSettings.captureSubmissionSettings()).toBeNull();
  await act(async () => settings.permissionMode.select("read_only"));
  expect(settings.permissionMode.mode).toBe("read_only");
  expect(settings.draftSessionSettings.captureSubmissionSettings()).toEqual({ permissionMode: "read_only" });
  expect(save).not.toHaveBeenCalled();
  await act(async () => root.render(<Harness slot="draft:b" />));
  expect(settings.permissionMode.mode).toBe("workspace_write");
});

it("saves existing sessions once and keeps the receipt until the projection changes", async () => {
  let finish!: (value: SessionPermissionModeUpdateResponse) => void;
  save.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await act(async () => root.render(<Harness sessionId="session-a" authority={authority()} />));
  await act(async () => { settings.permissionMode.select("full_access"); settings.permissionMode.select("full_access"); });
  expect(save).toHaveBeenCalledExactlyOnceWith("session-a", "full_access");
  expect(settings.permissionMode.saving).toBe(true);
  const barrier = settings.permissionMode.flush();
  await act(async () => { finish(receipt("full_access")); expect(await barrier).toBe(true); });
  expect(settings.permissionMode.mode).toBe("full_access");
  await act(async () => root.render(<Harness sessionId="session-a" authority={authority("read_only")} />));
  expect(settings.permissionMode.mode).toBe("read_only");
});

it("keeps the saved mode and reports a failed write", async () => {
  save.mockRejectedValueOnce(new Error("disk full"));
  await act(async () => root.render(<Harness sessionId="session-a" authority={authority()} />));
  await act(async () => { settings.permissionMode.select("read_only"); expect(await settings.permissionMode.flush()).toBe(false); });
  expect(settings.permissionMode.mode).toBe("workspace_write");
  expect(toast.error).toHaveBeenCalledTimes(1);
});

it("is unavailable until the session owner loads", async () => {
  await act(async () => root.render(<Harness sessionId="session-a" authority={null} />));
  expect(settings.permissionMode.available).toBe(false);
  await act(async () => settings.permissionMode.select("read_only"));
  expect(save).not.toHaveBeenCalled();
});
