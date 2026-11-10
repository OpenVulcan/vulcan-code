// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { toast } from "@/ui-kit/primitives/toast";
import { DefaultPermissionModeSettingsSection } from "@/components/features/settings/default-permission-mode-settings";
import type { PermissionMode } from "@/service/permission-mode";
import { saveDefaultPermissionMode, setSessionPermissionMode, setSessionUnattended, type SessionPermissionModeUpdateResponse } from "@/service/permission-mode-service";
import { useComposerSessionSettings } from "./use-composer-answer-mode";
import type { PermissionModeOwner } from "./use-composer-permission-mode";
import { PermissionJudgeEntryContext } from "../permission-judge-entry-context";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { error: vi.fn(), info: vi.fn() } }));
vi.mock("@/service/permission-mode-service", () => ({
  saveDefaultPermissionMode: vi.fn().mockResolvedValue(null),
  setSessionPermissionMode: vi.fn(),
  setSessionUnattended: vi.fn(),
}));

// root/host isolate actual hook lifetimes; only RPC and text catalogs are synthetic.
// root/host 隔离真实钩子生命周期；仅 RPC 与文案目录使用合成边界。
let root: Root;
let host: HTMLDivElement;
let settings: ReturnType<typeof useComposerSessionSettings>;
const save = vi.mocked(setSessionPermissionMode);
const saveUnattended = vi.mocked(setSessionUnattended);

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
  return { sessionId, generation: 1, mode, unattended: null };
}

/** Build a durable service receipt for one session.
 * 为一个会话构建可靠服务回执。 */
function receipt(mode: PermissionMode, sessionId = "session-a"): SessionPermissionModeUpdateResponse {
  return { session_id: sessionId, revision: 2, permission_mode: mode, unattended: null };
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

it("uses the published permission default for new drafts while keeping existing session authority", async () => {
  // judge models a saved default without a configured backend.
  // judge 模拟保存了默认值但尚未配置判定后端。
  const judge = { backend: null, openSettings: vi.fn(), openGuide: vi.fn() };
  await act(async () => root.render(<PermissionJudgeEntryContext.Provider value={{ ...judge, defaultPermissionMode: "full_access" }}>
    <Harness />
  </PermissionJudgeEntryContext.Provider>));
  expect(settings.permissionMode.mode).toBe("full_access");
  expect(settings.draftSessionSettings.captureSubmissionSettings()).toBeNull();
  await act(async () => root.render(<PermissionJudgeEntryContext.Provider value={{ ...judge, defaultPermissionMode: "auto_approve" }}>
    <Harness />
  </PermissionJudgeEntryContext.Provider>));
  expect(settings.permissionMode.mode).toBe("workspace_write");
  await act(async () => root.render(<PermissionJudgeEntryContext.Provider value={{ ...judge, defaultPermissionMode: "full_access" }}>
    <Harness sessionId="session-a" authority={authority("read_only")} />
  </PermissionJudgeEntryContext.Provider>));
  expect(settings.permissionMode.mode).toBe("read_only");
  expect(save).not.toHaveBeenCalled();
});

it("requires confirmation before saving a full-access default and disables an unavailable judge", async () => {
  await act(async () => root.render(<DefaultPermissionModeSettingsSection judgeConfigured={false} />));
  // choices resolve the real settings controls by their mocked translation keys.
  // choices 按模拟的目录键定位真实设置控件。
  const choices = Array.from(host.querySelectorAll<HTMLButtonElement>('[role="radio"]'));
  expect(choices.find((button) => button.textContent === "permissionMode.autoApprove")?.disabled).toBe(true);
  await act(async () => choices.find((button) => button.textContent === "permissionMode.fullAccess")!.click());
  expect(saveDefaultPermissionMode).not.toHaveBeenCalled();
  // confirm and acknowledge are the real full-access guard controls.
  // confirm 与 acknowledge 是真实的完全权限确认控件。
  const confirm = document.querySelector<HTMLButtonElement>("[data-full-access-confirm]")!;
  const acknowledge = document.querySelector<HTMLButtonElement>("[data-full-access-acknowledge]")!;
  expect(confirm.disabled).toBe(true);
  await act(async () => acknowledge.click());
  await act(async () => confirm.click());
  expect(saveDefaultPermissionMode).toHaveBeenCalledExactlyOnceWith("full_access");
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

it("saves the confirmed unattended policy and keeps it until the projection changes", async () => {
  saveUnattended.mockResolvedValueOnce({ session_id: "session-a", revision: 2, unattended: { deny_policy: "no push" } });
  await act(async () => root.render(<Harness sessionId="session-a" authority={authority()} />));
  expect(settings.permissionMode.unattended.value).toBeNull();
  await act(async () => { expect(await settings.permissionMode.unattended.apply({ deny_policy: "no push" })).toBe(true); });
  expect(saveUnattended).toHaveBeenCalledExactlyOnceWith("session-a", { deny_policy: "no push" });
  expect(settings.permissionMode.unattended.value).toEqual({ deny_policy: "no push" });
  await act(async () => root.render(<Harness sessionId="session-a" authority={{ ...authority(), unattended: { deny_policy: "x" } }} />));
  expect(settings.permissionMode.unattended.value).toEqual({ deny_policy: "x" });
});

it("drops unattended mode with the receipt of a mode change", async () => {
  const owner = { ...authority("auto_approve"), unattended: { deny_policy: "no push" } };
  save.mockResolvedValueOnce(receipt("workspace_write"));
  await act(async () => root.render(<Harness sessionId="session-a" authority={owner} />));
  expect(settings.permissionMode.unattended.value).toEqual({ deny_policy: "no push" });
  await act(async () => { settings.permissionMode.select("workspace_write"); expect(await settings.permissionMode.flush()).toBe(true); });
  expect(settings.permissionMode.mode).toBe("workspace_write");
  expect(settings.permissionMode.unattended.value).toBeNull();
});

it("offers unattended mode only to saved sessions", async () => {
  await act(async () => root.render(<Harness />));
  expect(settings.permissionMode.unattended.available).toBe(false);
  await act(async () => { expect(await settings.permissionMode.unattended.apply({ deny_policy: "" })).toBe(false); });
  expect(saveUnattended).not.toHaveBeenCalled();
});

it("is unavailable until the session owner loads", async () => {
  await act(async () => root.render(<Harness sessionId="session-a" authority={null} />));
  expect(settings.permissionMode.available).toBe(false);
  await act(async () => settings.permissionMode.select("read_only"));
  expect(save).not.toHaveBeenCalled();
});
