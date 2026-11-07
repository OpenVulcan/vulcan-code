// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { requestNewConversationWithText } from "@/components/composer/composer-external-commands";
import { prepareSelfFixWorkspace } from "@/service/project-actions-service";
import SelfFixDialog, { type SelfFixProblem } from "./self-fix-dialog";

vi.mock("@/i18n", () => ({
  useI18n: () => ({ locale: "en-US", t: (key: string) => key }),
  formatMessage: (template: string) => template,
}));
vi.mock("@/service/project-actions-service", () => ({ prepareSelfFixWorkspace: vi.fn(), browseProjectDirectories: vi.fn() }));
vi.mock("@/components/composer/composer-external-commands", () => ({ requestNewConversationWithText: vi.fn() }));
// The folder picker stands in as one button that confirms a fixed directory.
// 文件夹选择器以一个确认固定目录的按钮代替。
vi.mock("@/components/project/folder-picker-dialog", () => ({
  ProjectFolderPickerDialog: (props: { onConfirm: (path: string) => Promise<void> }) => (
    <button type="button" onClick={() => void props.onConfirm("D:/work")}>pick-folder</button>
  ),
}));

let host: HTMLDivElement;
let root: Root;

// PROBLEM is a sent bug report without error facts.
// PROBLEM 是一份已发送、不含错误事实的问题报告。
const PROBLEM: SelfFixProblem = { title: "Crash", body: "It crashes.", error: null, feedbackId: "fb_1" };

// button finds an action by its catalog key; a missing key means the expected action may have been renamed.
// button 按目录键查找操作；找不到说明预期操作可能已改名。
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
  vi.mocked(prepareSelfFixWorkspace).mockReset();
  vi.mocked(requestNewConversationWithText).mockReset();
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

it("opens a draft in the prepared project with the self-fix skill on, only after a folder is chosen", async () => {
  vi.mocked(prepareSelfFixWorkspace).mockResolvedValue({ projectId: "p1", path: "D:/work/vulcan-code", existingCheckout: false, installedVersion: "0.1.9" });
  const onStarted = vi.fn();
  await act(async () => root.render(<SelfFixDialog problem={PROBLEM} onClose={vi.fn()} onStarted={onStarted} />));
  expect(button("feedback.selfFix.start").disabled).toBe(true);

  await act(async () => button("feedback.selfFix.choose").click());
  await act(async () => button("pick-folder").click());
  await act(async () => button("feedback.selfFix.start").click());

  expect(prepareSelfFixWorkspace).toHaveBeenCalledWith("D:/work", "vulcan-code");
  const [prompt, target] = vi.mocked(requestNewConversationWithText).mock.calls[0];
  expect(target).toEqual({ projectId: "p1", promptSkillOverrides: { "vulcan-self-fix": "enabled" } });
  expect(prompt).toContain("feedback.selfFix.promptFeedback");
  expect(prompt).not.toContain("feedback.selfFix.promptCheckout");
  expect(onStarted).toHaveBeenCalledTimes(1);
});

it("keeps the window open and opens nothing when the directory is refused", async () => {
  vi.mocked(prepareSelfFixWorkspace).mockRejectedValue(new Error("The target directory is not empty"));
  const onStarted = vi.fn();
  await act(async () => root.render(<SelfFixDialog problem={PROBLEM} onClose={vi.fn()} onStarted={onStarted} />));
  await act(async () => button("feedback.selfFix.choose").click());
  await act(async () => button("pick-folder").click());
  await act(async () => button("feedback.selfFix.start").click());

  expect(requestNewConversationWithText).not.toHaveBeenCalled();
  expect(onStarted).not.toHaveBeenCalled();
  expect(document.body.textContent).toContain("The target directory is not empty");
});
