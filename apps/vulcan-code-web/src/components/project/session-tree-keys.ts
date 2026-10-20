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
