// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider, readLocaleMessage } from "@/i18n";
import { ensureLocaleCatalog } from "@/i18n-catalogs";
import { DedicatedDraftStrip } from "./dedicated-draft-strip";
import { INITIAL_DEDICATED_DRAFT } from "./dedicated-draft-store";
import type { DraftComposerContextValue } from "./draft-composer-context";

// host and root own only the isolated draft strip; the browser Select renders its list in the document body.
// host 与 root 仅承载本隔离草稿标签行；浏览器选择器把列表渲染到 document body。
const host = document.createElement("div");
let root: ReturnType<typeof createRoot>;

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  Element.prototype.scrollIntoView = vi.fn();
  document.body.append(host);
  root = createRoot(host);
  // The picker row is located by its real localized label, so the catalog must be published first.
  // 选择器行按其真实本地化标签定位，因此必须先发布文案目录。
  await ensureLocaleCatalog("zh-CN");
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

// option resolves one visible choice by its user-facing text instead of a structural position.
// option 按用户可见文本解析某个可见选项，而不依赖结构位置。
function option(label: string): HTMLElement {
  const match = [...document.querySelectorAll<HTMLElement>('[role="option"]')]
    .find((candidate) => candidate.textContent?.trim() === label);
  if (!match) throw new Error(`Expected picker option ${label}; check the current picker rows`);
  return match;
}

// choose opens the project picker by keyboard and activates the exact visible row.
// choose 通过键盘打开项目选择器并激活准确的可见行。
async function choose(label: string): Promise<void> {
  const trigger = host.querySelector<HTMLButtonElement>('[role="combobox"]')!;
  await act(async () => { trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })); });
  await act(async () => { option(label).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); });
}

// mount renders the strip over one draft whose project choices are observed callbacks.
// mount 在单个草稿之上渲染标签行，草稿的项目选择由可观察回调承担。
async function mount(draft: DraftComposerContextValue): Promise<void> {
  await act(async () => root.render(
    <I18nProvider initialLocale="zh-CN">
      <DedicatedDraftStrip
        disabled={false}
        draft={draft}
        onChange={() => {}}
        state={INITIAL_DEDICATED_DRAFT}
      />
    </I18nProvider>,
  ));
}

it("opens the folder picker from the project picker instead of choosing a project", async () => {
  const onChangeProject = vi.fn();
  const onCreateProject = vi.fn();
  await mount({
    draftId: "draft-a",
    projectId: "project-a",
    projects: [{ id: "project-a", name: "Project A" }],
    submissionPending: false,
    onChangeProject,
    onCreateProject,
  });
  await choose(readLocaleMessage("zh-CN", "sidebar.newProject"));
  expect(onCreateProject).toHaveBeenCalledOnce();
  expect(onChangeProject).not.toHaveBeenCalled();
});

it("moves the draft when one existing project row is chosen", async () => {
  const onChangeProject = vi.fn();
  const onCreateProject = vi.fn();
  await mount({
    draftId: "draft-a",
    projectId: "project-a",
    projects: [{ id: "project-a", name: "Project A" }, { id: "project-b", name: "Project B" }],
    submissionPending: false,
    onChangeProject,
    onCreateProject,
  });
  await choose("Project B");
  expect(onChangeProject).toHaveBeenCalledExactlyOnceWith("project-b");
  expect(onCreateProject).not.toHaveBeenCalled();
});
