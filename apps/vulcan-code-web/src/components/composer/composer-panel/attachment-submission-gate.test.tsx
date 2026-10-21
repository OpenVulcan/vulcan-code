// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useComposerAttachmentSubmissionGate } from "./attachment-submission-gate";
import type { ComposerAttachmentPreparationController } from "./use-composer-attachment-preparation";
import type { ComposerAttachmentPreparationSummary } from "@/types";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
// toastInfo records barrier notices without mounting the global toaster.
// toastInfo 记录屏障提示，不挂载全局通知组件。
const toastInfo = vi.hoisted(() => vi.fn());
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { info: toastInfo } }));

// host and root isolate one gate; send records the exact submissions it allows.
// host 与 root 隔离单个闸门；send 记录其允许的精确提交。
let host: HTMLDivElement;
let root: Root;
const send = vi.fn(async () => {});
let admit: () => boolean = () => false;

/** Builds one synthetic plan row with the given delivery reason.
 * 使用给定投递原因构建一条合成规划记录。
 */
function row(attachmentId: string, reason: string): ComposerAttachmentPreparationSummary {
  return { attachmentId, generation: 1, transferId: `t-${attachmentId}`, deliveryPlan: "metadata_only", phase: "metadata_only", blocksSubmission: false, routeRevision: "r", reason, error: null };
}

/** Mounts the gate over a synthetic controller and captures its admit action.
 * 在合成控制器上挂载闸门并捕获其 admit 动作。
 */
async function mount(preparations: Record<string, ComposerAttachmentPreparationSummary>, blocksSubmission = false) {
  const controller = { preparations, blocksSubmission, blockingCount: blocksSubmission ? 1 : 0 } as ComposerAttachmentPreparationController;
  function Probe() {
    const gate = useComposerAttachmentSubmissionGate(controller, send);
    admit = gate.admit;
    return <>{gate.dialog}</>;
  }
  await act(async () => root.render(<Probe />));
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it("admits ordinary attachments and holds a pending preparation barrier", async () => {
  await mount({ a: row("a", "inline_threshold") });
  expect(admit()).toBe(true);
  await mount({}, true);
  expect(admit()).toBe(false);
  expect(toastInfo).toHaveBeenCalledTimes(1);
});

it("sends over-budget attachments only after explicit confirmation", async () => {
  await mount({ a: row("a", "encoded_budget"), b: row("b", "encoded_budget") });
  let admitted = true;
  await act(async () => { admitted = admit(); });
  expect(admitted).toBe(false);
  expect(send).not.toHaveBeenCalled();
  const dialog = document.querySelector("[role='alertdialog']");
  expect(dialog?.textContent).toContain("composer.attachment.overflowTitle");
  const confirm = Array.from(dialog!.querySelectorAll("button")).find((button) => button.textContent === "composer.attachment.overflowConfirm")!;
  await act(async () => confirm.click());
  expect(send).toHaveBeenCalledTimes(1);
  expect(document.querySelector("[role='alertdialog']")).toBeNull();
});
