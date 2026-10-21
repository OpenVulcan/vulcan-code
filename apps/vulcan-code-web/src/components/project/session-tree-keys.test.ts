// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { handleSessionStepKey, handleSessionTreeKey } from "./session-tree-keys";

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

describe("session step keys", () => {
  // buildSessions lists a pinned copy of session b ahead of the project rows a, b and c, with b open.
  // buildSessions 在项目行 a、b、c 之前列出会话 b 的置顶副本，并打开 b。
  function buildSessions(): { list: HTMLElement; opened: string[] } {
    const list = document.createElement("div");
    list.innerHTML = ["b", "a", "b", "c"].map((id) => `<button data-conversation-id="${id}"></button>`).join("");
    document.body.append(list);
    const opened: string[] = [];
    for (const button of list.querySelectorAll<HTMLElement>("button")) {
      if (button.dataset.conversationId === "b") button.setAttribute("aria-current", "true");
      button.addEventListener("click", () => opened.push(button.dataset.conversationId!));
    }
    Element.prototype.scrollIntoView = () => {};
    return { list, opened };
  }

  // step sends one Alt+arrow key from the given target and returns whether it was used.
  // step 从指定目标发送一次 Alt+方向键，并返回该按键是否被使用。
  function step(list: HTMLElement, key: string, target: EventTarget = document.body, modifiers: Partial<KeyboardEvent> = {}): boolean {
    return handleSessionStepKey({ key, altKey: true, ctrlKey: false, metaKey: false, shiftKey: false, isComposing: false, defaultPrevented: false, target, preventDefault: vi.fn(), ...modifiers }, list);
  }

  it("opens the neighbour of the open session in list order, counting a repeated session once", () => {
    const { list, opened } = buildSessions();
    expect(step(list, "ArrowDown")).toBe(true);
    expect(step(list, "ArrowUp")).toBe(false);
    expect(opened).toEqual(["a"]);
  });

  it("leaves keys that belong elsewhere alone", () => {
    const { list, opened } = buildSessions();
    const select = document.createElement("select");
    document.body.append(select);
    expect(step(list, "ArrowDown", select)).toBe(false);
    expect(step(list, "ArrowDown", document.body, { altKey: false })).toBe(false);
    expect(step(list, "ArrowDown", document.body, { defaultPrevented: true })).toBe(false);
    expect(opened).toEqual([]);
  });
});
