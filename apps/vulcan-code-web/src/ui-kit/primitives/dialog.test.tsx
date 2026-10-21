// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { Dialog, DialogContent, DialogTitle } from "./dialog";

// A dialog opened from code (a shortcut, not a DialogTrigger) whose input takes focus with autoFocus must
// still give focus back to whatever held it before opening.
// 由代码（快捷键而非 DialogTrigger）打开、且其输入框以 autoFocus 取得焦点的对话框，关闭后仍须把焦点还给打开前的元素。
it("returns focus to the opener when a child autofocuses", async () => {
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // render mounts an outside opener next to a controlled dialog.
  // render 在受控对话框旁挂载一个外部打开者。
  const render = (open: boolean) => root.render(<>
    <button data-opener type="button">Opener</button>
    <Dialog open={open} onOpenChange={() => {}}>
      <DialogContent><DialogTitle>Title</DialogTitle><input autoFocus aria-label="search" /></DialogContent>
    </Dialog>
  </>);
  try {
    await act(async () => render(false));
    const opener = host.querySelector<HTMLButtonElement>("[data-opener]")!;
    opener.focus();
    await act(async () => render(true));
    expect(document.activeElement?.getAttribute("aria-label")).toBe("search");
    await act(async () => render(false));
    expect(document.activeElement).toBe(opener);
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
  }
});
