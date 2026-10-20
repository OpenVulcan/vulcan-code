// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { toast } from "@/ui-kit/primitives/toast";
import type { AnswerMode, AnswerModeOwner } from "@/service/answer-mode";
import { setSessionAnswerMode, type AnswerModeUpdateResponse } from "@/service/answer-mode-service";
import { useComposerSessionSettings } from "./use-composer-answer-mode";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { error: vi.fn(), info: vi.fn() } }));
vi.mock("@/service/answer-mode-service", () => ({ setSessionAnswerMode: vi.fn() }));

// root/host isolate actual hook lifetimes; only RPC and text catalogs are synthetic.
// root/host 隔离真实钩子生命周期；仅 RPC 与文案目录使用合成边界。
let root: Root;
let host: HTMLDivElement;
let settings: ReturnType<typeof useComposerSessionSettings>;
const save = vi.mocked(setSessionAnswerMode);

/** Render the actual answer-mode hook and draft capture for one exact owner.
 * 为一个精确归属渲染真实回答模式钩子与草稿捕获。 */
function Harness({ sessionId = null, authority = null, slot = "draft:a", globalDefault = "standard" }: {
  sessionId?: string | null; authority?: AnswerModeOwner | null; slot?: string; globalDefault?: AnswerMode;
}) {
  settings = useComposerSessionSettings(slot, sessionId, authority, globalDefault, null);
  return null;
}

/** Build a complete persisted owner from a synthetic session and revision.
 * 从合成会话与修订构建完整持久归属。 */
function authority(sessionId = "session-a", mode: AnswerMode = "standard", revision = 0): AnswerModeOwner {
  return { sessionId, generation: 1, mode: { mode, revision, prepared: null } };
}

/** Build a durable service receipt carrying the captured owner preference.
 * 构建携带捕获归属偏好的可靠服务回执。 */
function receipt(owner: AnswerModeOwner, conflict = false): AnswerModeUpdateResponse {
  return { session_id: owner.sessionId, window_generation: owner.generation,
    revision: owner.mode.revision + 1, answer_mode: owner.mode, conflict };
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.clearAllMocks();
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it("shows the global default for drafts and captures only explicit choices", async () => {
  await act(async () => root.render(<Harness globalDefault="interactive" />));
  expect(settings.answerMode.mode).toBe("interactive");
  expect(settings.draftSessionSettings.captureSubmissionSettings()).toBeNull();
  await act(async () => settings.answerMode.select("standard"));
  expect(settings.draftSessionSettings.captureSubmissionSettings()).toEqual({ answerMode: "standard" });
  expect(save).not.toHaveBeenCalled();
  await act(async () => root.render(<Harness slot="draft:b" />));
  expect(settings.answerMode.mode).toBe("standard");
  expect(settings.draftSessionSettings.captureSubmissionSettings()).toBeNull();
});

it("waits for durable acknowledgement, deduplicates selections, and adopts newer shared state", async () => {
  let finish!: (value: AnswerModeUpdateResponse) => void;
  save.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await act(async () => root.render(<React.StrictMode><Harness sessionId="session-a" authority={authority()} /></React.StrictMode>));
  await act(async () => { settings.answerMode.select("interactive"); settings.answerMode.select("interactive"); });
  expect(save).toHaveBeenCalledExactlyOnceWith("session-a", "interactive", 0);
  expect(settings.answerMode.mode).toBe("standard");
  expect(settings.answerMode.saving).toBe(true);
  const barrier = settings.answerMode.flush();
  await act(async () => { finish(receipt(authority("session-a", "interactive", 1))); expect(await barrier).toBe(true); });
  expect(settings.answerMode.mode).toBe("interactive");
  expect(settings.answerMode.awaitingRequest).toBe(true);
  await act(async () => root.render(<React.StrictMode><Harness sessionId="session-a" authority={authority("session-a", "standard", 2)} /></React.StrictMode>));
  expect(settings.answerMode.mode).toBe("standard");
});

it("preserves prior state on failure and reloads conflicts without silently retrying", async () => {
  save.mockRejectedValueOnce(new Error("disk full"));
  await act(async () => root.render(<Harness sessionId="session-a" authority={authority()} />));
  await act(async () => { settings.answerMode.select("interactive"); expect(await settings.answerMode.flush()).toBe(false); });
  expect(settings.answerMode.mode).toBe("standard");
  expect(toast.error).toHaveBeenCalledOnce();
  save.mockResolvedValueOnce(receipt(authority("session-a", "interactive", 3), true));
  await act(async () => { settings.answerMode.select("interactive"); expect(await settings.answerMode.flush()).toBe(false); });
  expect(settings.answerMode.mode).toBe("interactive");
  expect(toast.info).toHaveBeenCalledOnce();
  expect(save).toHaveBeenCalledTimes(2);
});

it("ignores late receipts even when navigating from A to B and back to A", async () => {
  let finish!: (value: AnswerModeUpdateResponse) => void;
  save.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await act(async () => root.render(<Harness slot="a" sessionId="session-a" authority={authority()} />));
  await act(async () => settings.answerMode.select("interactive"));
  const pending = settings.answerMode.flush();
  await act(async () => root.render(<Harness slot="b" sessionId="session-b" authority={authority("session-b")} />));
  await act(async () => root.render(<Harness slot="a" sessionId="session-a" authority={authority()} />));
  expect(settings.answerMode.saving).toBe(false);
  await act(async () => { finish(receipt(authority("session-a", "interactive", 1))); expect(await pending).toBe(false); });
  expect(settings.answerMode.mode).toBe("standard");
});
