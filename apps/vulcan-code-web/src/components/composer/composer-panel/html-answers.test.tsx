// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { setSessionHtmlAnswerMode, type HtmlAnswerModeUpdateResponse } from "@/service/html-answers-service";
import type { HtmlAnswerModeOwner } from "@/service/html-answer-mode";
import { useComposerHtmlAnswers } from "./use-composer-html-answers";
import { useComposerDraftSessionSettings } from "./use-composer-draft-session-settings";
import { HtmlAnswerToggle } from "./html-answer-toggle";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), info: vi.fn() } }));
vi.mock("@/service/html-answers-service", () => ({ setSessionHtmlAnswerMode: vi.fn() }));

// root/host isolate actual hook and button lifetimes; only RPC and text catalogs are synthetic.
// root/host 隔离真实钩子与按钮生命周期；仅 RPC 与文案目录使用合成边界。
let root: Root;
let host: HTMLDivElement;
let controller: ReturnType<typeof useComposerHtmlAnswers>;
let draft: ReturnType<typeof useComposerDraftSessionSettings>;
const save = vi.mocked(setSessionHtmlAnswerMode);

/** Render the actual preference hook, draft capture and button for one exact owner.
 * 为一个精确归属渲染真实偏好钩子、草稿捕获与按钮。 */
function Harness({ sessionId = null, authority = null, slot = "draft:a" }: {
  sessionId?: string | null; authority?: HtmlAnswerModeOwner | null; slot?: string;
}) {
  draft = useComposerDraftSessionSettings(slot, sessionId);
  controller = useComposerHtmlAnswers({ scopeKey: slot, sessionId, authority,
    draftEnabled: draft.htmlAnswersEnabled, setDraftEnabled: draft.setHtmlAnswersEnabled });
  return <HtmlAnswerToggle controller={controller} disabled={false} />;
}

/** Build a complete persisted owner from a synthetic session and revision.
 * 从合成会话与修订构建完整持久归属。 */
function authority(sessionId = "session-a", enabled = false, revision = 0): HtmlAnswerModeOwner {
  return { sessionId, generation: 1, mode: { enabled, revision, prepared: null } };
}

/** Build a durable service receipt carrying the captured owner preference.
 * 构建携带捕获归属偏好的可靠服务回执。 */
function receipt(owner: HtmlAnswerModeOwner, conflict = false): HtmlAnswerModeUpdateResponse {
  return { session_id: owner.sessionId, window_generation: owner.generation,
    revision: owner.mode.revision + 1, html_answers: owner.mode, conflict };
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.clearAllMocks();
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it("captures draft HTML preference with first submission and isolates draft slots", async () => {
  await act(async () => root.render(<Harness />));
  expect(host.querySelector("button")?.getAttribute("aria-pressed")).toBe("false");
  await act(async () => host.querySelector("button")!.click());
  expect(draft.captureSubmissionSettings()).toEqual({ htmlAnswersEnabled: true });
  expect(save).not.toHaveBeenCalled();
  await act(async () => root.render(<Harness slot="draft:b" />));
  expect(controller.enabled).toBe(false);
  await act(async () => root.render(<Harness />));
  expect(controller.enabled).toBe(true);
});

it("waits for durable acknowledgement, deduplicates clicks, and adopts newer shared state", async () => {
  let finish!: (value: HtmlAnswerModeUpdateResponse) => void;
  save.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await act(async () => root.render(<React.StrictMode><Harness sessionId="session-a" authority={authority()} /></React.StrictMode>));
  await act(async () => { controller.toggle(); controller.toggle(); });
  expect(save).toHaveBeenCalledExactlyOnceWith("session-a", true, 0);
  expect(controller.enabled).toBe(false);
  expect(controller.saving).toBe(true);
  const barrier = controller.flush();
  await act(async () => { finish(receipt(authority("session-a", true, 1))); expect(await barrier).toBe(true); });
  expect(controller.enabled).toBe(true);
  expect(controller.awaitingRequest).toBe(true);
  await act(async () => root.render(<React.StrictMode><Harness sessionId="session-a" authority={authority("session-a", false, 2)} /></React.StrictMode>));
  expect(controller.enabled).toBe(false);
});

it("preserves prior state on failure and reloads conflicts without silently retrying", async () => {
  save.mockRejectedValueOnce(new Error("disk full"));
  await act(async () => root.render(<Harness sessionId="session-a" authority={authority()} />));
  await act(async () => { controller.toggle(); expect(await controller.flush()).toBe(false); });
  expect(controller.enabled).toBe(false);
  expect(toast.error).toHaveBeenCalledOnce();
  save.mockResolvedValueOnce(receipt(authority("session-a", true, 3), true));
  await act(async () => { controller.toggle(); expect(await controller.flush()).toBe(false); });
  expect(controller.enabled).toBe(true);
  expect(toast.info).toHaveBeenCalledOnce();
  expect(save).toHaveBeenCalledTimes(2);
});

it("ignores late receipts even when navigating from A to B and back to A", async () => {
  let finish!: (value: HtmlAnswerModeUpdateResponse) => void;
  save.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await act(async () => root.render(<Harness slot="a" sessionId="session-a" authority={authority()} />));
  await act(async () => controller.toggle());
  const pending = controller.flush();
  await act(async () => root.render(<Harness slot="b" sessionId="session-b" authority={authority("session-b")} />));
  await act(async () => root.render(<Harness slot="a" sessionId="session-a" authority={authority()} />));
  expect(controller.saving).toBe(false);
  await act(async () => { finish(receipt(authority("session-a", true, 1))); expect(await pending).toBe(false); });
  expect(controller.enabled).toBe(false);
});
