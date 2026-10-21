// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { composerListOptionId, useComposerTextareaKeys } from "./use-composer-textarea-keys";

type Result = ReturnType<typeof useComposerTextareaKeys>;

// renderKeys mounts the hook with the slash list open or closed and returns what it reports.
// renderKeys 在 slash 列表打开或关闭时挂载该 hook，并返回其报告的内容。
async function renderKeys(slashPanelVisible: boolean): Promise<Result> {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  let result: Result | null = null;
  function Harness() {
    const textareaRef = React.useRef<HTMLTextAreaElement | null>(null);
    result = useComposerTextareaKeys({
      input: "/mo",
      setInput: () => {},
      textareaRef,
      recallScope: "test",
      slotKey: "slot",
      workspacePath: null,
      projectId: null,
      conversationId: null,
      slashPanelVisible,
      slashActiveIndex: 2,
      onKeyDown: () => {},
    });
    return null;
  }
  const root = createRoot(document.createElement("div"));
  await act(async () => root.render(<Harness />));
  await act(async () => root.unmount());
  vi.unstubAllGlobals();
  return result!;
}

it("points the textarea at the open list and its highlighted row, and at nothing when no list is open", async () => {
  const open = await renderKeys(true);
  expect(open.textareaListbox).toEqual({
    "aria-autocomplete": "list",
    "aria-controls": open.listboxIds.slash,
    "aria-activedescendant": composerListOptionId(open.listboxIds.slash, 2),
  });
  expect(open.listboxIds.slash).not.toBe(open.listboxIds.path);
  const closed = await renderKeys(false);
  expect(closed.textareaListbox["aria-controls"]).toBeUndefined();
  expect(closed.textareaListbox["aria-activedescendant"]).toBeUndefined();
});
