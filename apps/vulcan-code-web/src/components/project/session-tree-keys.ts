import { moveInList } from "@/ui-kit/behavior/roving";

// SESSION_TREE_ROW_SELECTOR names the rows arrow keys step through; each row's action buttons stay on Tab.
// SESSION_TREE_ROW_SELECTOR 指定方向键逐行经过的行；每行的操作按钮仍由 Tab 到达。
export const SESSION_TREE_ROW_SELECTOR = ".vk-group-row, .vk-tree-row, .vk-tree-session, .vk-tree-more";

// handleSessionTreeKey moves focus between the visible rows of the session tree: Up and Down step, Home and End
// jump, Right opens a closed group or folder and Left closes an open one. Keys pressed on anything other than a
// row (a grip being dragged, an action button) are left alone. Returns whether the key was used.
// handleSessionTreeKey 在会话树的可见行之间移动焦点：上下键逐行移动，Home 与 End 跳到首尾，右键展开关闭的分组或
// 文件夹，左键收起已展开的。在行以外的元素（拖动中的手柄、操作按钮）上按下的键不作处理。返回该按键是否被使用。
export function handleSessionTreeKey(
  event: { key: string; target: EventTarget | null; currentTarget: HTMLElement; altKey: boolean; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; preventDefault: () => void },
): boolean {
  if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return false;
  const row = event.target instanceof HTMLElement && event.target.matches(SESSION_TREE_ROW_SELECTOR) ? event.target : null;
  if (!row) return false;
  if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
    const expanded = row.getAttribute("aria-expanded");
    if (expanded === (event.key === "ArrowRight" ? "false" : "true")) {
      event.preventDefault();
      row.click();
      return true;
    }
    return false;
  }
  if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return false;
  return moveInList(event, event.currentTarget, SESSION_TREE_ROW_SELECTOR);
}

// handleSessionStepKey opens the previous or next listed session on Alt+Up or Alt+Down from anywhere on the page,
// in the order the list shows them. A session listed twice (pinned and in its project) counts once, where it first
// appears. Keys that already belong to something else (a dialog, a select or combobox, a key another list
// instance took) are left alone. Returns whether the key was used.
// handleSessionStepKey 在页面任意位置按 Alt+上/下键时，按列表显示顺序打开上一个或下一个会话。出现两次的会话（置顶
// 与所属项目中）只按首次出现的位置计算。已属于其他用途的按键（对话框、下拉选择或组合框、已被另一个列表实例处理的
// 按键）不作处理。返回该按键是否被使用。
export function handleSessionStepKey(
  event: Pick<KeyboardEvent, "key" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey" | "isComposing" | "defaultPrevented" | "target" | "preventDefault">,
  list: HTMLElement,
): boolean {
  if (!event.altKey || event.ctrlKey || event.metaKey || event.shiftKey || event.isComposing || event.defaultPrevented) return false;
  if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return false;
  if (event.target instanceof Element && event.target.closest("[role=dialog], select, [role=combobox]")) return false;
  const seen = new Set<string>();
  const rows = [...list.querySelectorAll<HTMLElement>("[data-conversation-id]")].filter((row) => {
    const id = row.dataset.conversationId!;
    return seen.has(id) ? false : (seen.add(id), true);
  });
  const current = rows.findIndex((row) => row.getAttribute("aria-current") === "true");
  const next = current < 0
    ? rows[event.key === "ArrowDown" ? 0 : rows.length - 1]
    : rows[current + (event.key === "ArrowDown" ? 1 : -1)];
  if (!next) return false;
  event.preventDefault();
  next.click();
  next.scrollIntoView({ block: "nearest" });
  return true;
}
