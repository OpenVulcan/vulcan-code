// Carries the logical key and modifier state used by the workbench shortcut handlers.
// 表示工作台快捷键处理器所用的逻辑键与修饰状态。
type ShortcutEvent = Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">;

// Maps eligible modified keys to their workbench action or native text-editing action.
// 将可匹配的组合键映射到工作台操作或原生文本编辑操作。
// Callers still decide whether the matching action applies in their current scope.
// 调用方仍负责判断匹配的操作是否适用于当前作用域。
export function workbenchShortcutAction({ key, ctrlKey, metaKey, altKey, shiftKey }: ShortcutEvent) {
  if (shiftKey && key === "F10") return "contextMenu";
  if (!(ctrlKey || metaKey) || altKey) return null;
  switch (key.toLowerCase()) {
    case "+": case "=": return "textLarger";
    case "-": case "_": return "textSmaller";
    case "0": return "textDefault";
    case "enter": return "send";
    case "o": return shiftKey ? "newConversation" : null;
    case "k": return shiftKey ? null : "palette";
    case "m": return shiftKey ? null : "modelPicker";
    case "b": return shiftKey ? null : "sidebar";
    case "f": return shiftKey ? null : "find";
    case "g": return shiftKey ? null : "gotoLine";
    case "v": case "z": return "textEdit";
    case "a": case "c": case "x": case "y": return shiftKey ? null : "textEdit";
    default: return null;
  }
}
