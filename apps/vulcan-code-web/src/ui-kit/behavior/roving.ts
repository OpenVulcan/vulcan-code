import { focusElement } from "./core";

// Keyboard movement through a vertical list of focusable rows (menus and listboxes): arrows step, Home and
// End jump, and printable keys typed in quick succession jump to the next row whose text starts with them.
// 在纵向可聚焦行列表（菜单与列表框）中的键盘移动：方向键逐行移动，Home 与 End 跳到首尾，快速连续输入的可打印
// 字符跳到下一个以其开头的行。

// listRows returns the enabled rows of a list in DOM order.
// listRows 按 DOM 顺序返回列表中的可用行。
export function listRows(container: HTMLElement, selector: string): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(selector)).filter(
    (row) => !row.hasAttribute("data-disabled") && row.getAttribute("aria-disabled") !== "true",
  );
}

// typeahead keeps the characters typed recently across calls.
// typeahead 在多次调用之间保存最近输入的字符。
const typeahead = { text: "", at: 0 };

// TYPEAHEAD_RESET_MS is how long a pause ends one typed search.
// TYPEAHEAD_RESET_MS 是结束一次输入搜索所需的停顿时长。
const TYPEAHEAD_RESET_MS = 700;

// moveInList handles one key for the list and focuses the next row; returns whether the key was used.
// moveInList 为列表处理一次按键并聚焦目标行；返回该按键是否被使用。
export function moveInList(
  event: { key: string; ctrlKey?: boolean; metaKey?: boolean; altKey?: boolean; preventDefault: () => void },
  container: HTMLElement,
  selector: string,
  options: { loop?: boolean; onMove?: (row: HTMLElement) => void } = {},
): boolean {
  const rows = listRows(container, selector);
  if (rows.length === 0) return false;
  const active = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const index = active ? rows.findIndex((row) => row === active || row.contains(active)) : -1;
  let next: HTMLElement | undefined;
  switch (event.key) {
    case "ArrowDown":
      next = index < 0 ? rows[0] : rows[index + 1] ?? (options.loop ? rows[0] : rows[index]);
      break;
    case "ArrowUp":
      next = index < 0 ? rows[rows.length - 1] : rows[index - 1] ?? (options.loop ? rows[rows.length - 1] : rows[index]);
      break;
    case "Home":
    case "PageUp":
      next = rows[0];
      break;
    case "End":
    case "PageDown":
      next = rows[rows.length - 1];
      break;
    default: {
      if (event.key.length !== 1 || event.ctrlKey || event.metaKey || event.altKey) return false;
      const now = Date.now();
      // A space that does not continue a search activates the row instead.
      // 不属于连续搜索的空格改为激活当前行。
      if (event.key === " " && now - typeahead.at > TYPEAHEAD_RESET_MS) return false;
      typeahead.text = now - typeahead.at > TYPEAHEAD_RESET_MS ? event.key : typeahead.text + event.key;
      typeahead.at = now;
      const query = typeahead.text.toLocaleLowerCase();
      const label = (row: HTMLElement) => (row.dataset.textValue ?? row.textContent ?? "").trim().toLocaleLowerCase();
      // A repeated single character cycles through rows starting with it.
      // 重复输入同一字符时在以其开头的行之间循环。
      const ordered = [...rows.slice(index + 1), ...rows.slice(0, index + 1)];
      next = ordered.find((row) => label(row).startsWith(query))
        ?? (query.length > 1 && [...query].every((char) => char === query[0]) ? ordered.find((row) => label(row).startsWith(query[0])) : undefined);
      if (!next) return true;
    }
  }
  event.preventDefault();
  if (next) {
    focusElement(next);
    next.scrollIntoView?.({ block: "nearest" });
    options.onMove?.(next);
  }
  return true;
}
