// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { handleSessionTreeKey } from "./session-tree-keys";

// buildTree mounts a group, a folder with one session and a show-more row, each followed by an action button.
// buildTree 挂载一个分组、一个含一个会话的文件夹与一行“显示更多”，每行后跟一个操作按钮。
function buildTree(): HTMLElement {
  const list = document.createElement("div");
  list.innerHTML = `
    <button class="vk-group-row" aria-expanded="false" data-row="group"></button><button class="vk-icon-btn" data-row="group-action"></button>
    <button class="vk-tree-row" aria-expanded="true" data-row="folder"></button><button class="vk-icon-btn vk-tree-grip" data-row="grip"></button>
    <button class="vk-tree-session" data-row="session"></button><button class="vk-icon-btn" data-row="session-action"></button>
    <button class="vk-btn vk-tree-more" data-row="more"></button>`;
  document.body.append(list);
  return list;
}

// press dispatches one key on the focused element through the handler and returns whether it was used.
// press 通过处理器在当前焦点元素上派发一次按键，并返回该按键是否被使用。
function press(list: HTMLElement, key: string): boolean {
  return handleSessionTreeKey({ key, target: document.activeElement, currentTarget: list, altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, preventDefault: () => undefined });
}

const focused = () => (document.activeElement as HTMLElement | null)?.dataset.row;
const row = (list: HTMLElement, name: string) => list.querySelector<HTMLElement>(`[data-row="${name}"]`)!;

afterEach(() => { document.body.innerHTML = ""; });

describe("session tree keys", () => {
  it("steps over action buttons and jumps to either end", () => {
    const list = buildTree();
    row(list, "group").focus();
    expect(press(list, "ArrowDown")).toBe(true);
    expect(focused()).toBe("folder");
    press(list, "ArrowDown");
    expect(focused()).toBe("session");
    press(list, "End");
    expect(focused()).toBe("more");
    press(list, "ArrowUp");
    expect(focused()).toBe("session");
    press(list, "Home");
    expect(focused()).toBe("group");
  });

  it("opens closed rows with Right and closes open rows with Left only", () => {
    const list = buildTree();
    const group = row(list, "group");
    const folder = row(list, "folder");
    const groupClick = vi.fn();
    const folderClick = vi.fn();
    group.addEventListener("click", groupClick);
    folder.addEventListener("click", folderClick);
    group.focus();
    expect(press(list, "ArrowLeft")).toBe(false);
    expect(press(list, "ArrowRight")).toBe(true);
    folder.focus();
    expect(press(list, "ArrowRight")).toBe(false);
    expect(press(list, "ArrowLeft")).toBe(true);
    expect(groupClick).toHaveBeenCalledOnce();
    expect(folderClick).toHaveBeenCalledOnce();
  });

  it("ignores keys pressed outside a row, such as on a grip being dragged", () => {
    const list = buildTree();
    row(list, "grip").focus();
    expect(press(list, "ArrowDown")).toBe(false);
    expect(focused()).toBe("grip");
  });
});
