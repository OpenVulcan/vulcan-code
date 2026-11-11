import { formatMessage, type Translate } from "@/i18n";
import { workbenchShortcutAction } from "@/lib/workbench-shortcuts";

// The shortcut belongs to the current GUI profile, like the selected microphone.
// 快捷键与麦克风选择一样，保存在当前图形客户端。
const VOICE_INPUT_SHORTCUT_KEY = "vulcan.gui.voice-input-shortcut.v1";

// Use physical key codes so releasing a key still matches after the modifiers change.
// 使用物理键码，确保修饰键状态变化后仍能识别主键的释放。
const SHORTCUT_KEY = /^(?:Key[A-Z]|Digit[0-9]|Space|F(?:[1-9]|1[0-9]|2[0-4]))$/;

// Maps punctuation codes to key labels without depending on the user's keyboard layout.
// 将标点键码映射到键名，避免依赖用户的键盘布局。
const SYMBOL_KEYS: Readonly<Partial<Record<string, string>>> = {
  Backquote: "`", Minus: "-", Equal: "=", BracketLeft: "[", BracketRight: "]", Backslash: "\\",
  Semicolon: ";", Quote: "'", Comma: ",", Period: ".", Slash: "/", IntlBackslash: "\\",
  IntlRo: "\\", IntlYen: "¥", NumpadAdd: "+", NumpadSubtract: "-", NumpadMultiply: "*",
  NumpadDivide: "/", NumpadDecimal: ".", NumpadEqual: "=", NumpadComma: ",",
};

// These are the canonical modifier names and their corresponding keyboard-event fields.
// 这些是规范的修饰键名称及对应的键盘事件字段。
const MODIFIERS = [
  ["ctrlKey", "Control"],
  ["altKey", "Alt"],
  ["shiftKey", "Shift"],
  ["metaKey", "Meta"],
] as const;

// Returns whether `code` identifies a physical left or right modifier key.
// 返回 `code` 是否标识左侧或右侧的物理修饰键。
export function isVoiceInputModifierKey(code: string): boolean {
  return /^(?:Control|Alt|Shift|Meta)(?:Left|Right)$/.test(code);
}

// Validates the stored chord, keeping ordinary letters, digits, and spaces available for typing.
// 校验保存的组合键，保留普通字母、数字与空格用于输入。
function isVoiceInputShortcut(shortcut: string): boolean {
  if (isVoiceInputModifierKey(shortcut)) return true;
  // Separate the main physical key from its modifiers.
  // 将物理主键与修饰键分开。
  const parts = shortcut.split("+");
  // The last part is always the physical key code.
  // 最后一段始终是物理键码。
  const code = parts.pop();
  // Canonical order also rejects unknown or repeated modifiers.
  // 规范顺序同时拒绝未知或重复的修饰键。
  const ordered = MODIFIERS.map(([, name]) => name).filter((name) => parts.includes(name));
  return code !== undefined
    && (SHORTCUT_KEY.test(code) || Object.hasOwn(SYMBOL_KEYS, code))
    && parts.join("+") === ordered.join("+")
    && (code.startsWith("F") || ordered.some((name) => name !== "Shift"));
}

// Returns the displayed key for a physical shortcut code, including punctuation.
// 返回物理快捷键码对应的显示键名，包括标点。
function shortcutKeyLabel(code: string): string {
  return SYMBOL_KEYS[code] ?? code.replace(/^(?:Key|Digit)(?=[A-Z0-9]$)/, "");
}

// Checks a stored chord and its optional live event against the app's registered shortcuts.
// 对照应用已注册的快捷键检查存储组合键及可选的实时键盘事件。
export function voiceInputShortcutConflicts(shortcut: string, event?: KeyboardEvent): boolean {
  if (event && workbenchShortcutAction(event)) return true;
  const parts = shortcut.split("+");
  const code = parts.pop();
  if (!code || !parts.length) return false;
  // A shifted zero key can be punctuation; only the live layout knows whether it still emits zero.
  // 数字零键加 Shift 可能输入标点，实际是否仍输出数字零由当前布局决定。
  const shiftKey = parts.includes("Shift");
  if (shiftKey && code === "Digit0") return false;
  return Boolean(workbenchShortcutAction({
    key: shortcutKeyLabel(code), ctrlKey: parts.includes("Control"), metaKey: parts.includes("Meta"),
    altKey: parts.includes("Alt"), shiftKey,
  }));
}

// Returns the canonical chord for a supported key event, or null if the event cannot bind a shortcut.
// 返回键盘事件对应的规范组合键；事件不可绑定时返回 null。
export function voiceInputShortcutFromKey(event: KeyboardEvent): string | null {
  if (event.isComposing || event.key === "AltGraph" || event.getModifierState("AltGraph")) return null;
  if (isVoiceInputModifierKey(event.code)) {
    return MODIFIERS.some(([field, name]) => event[field] && !event.code.startsWith(name)) ? null : event.code;
  }
  // Compose the same representation for capture, storage, and keydown matching.
  // 捕获、存储与按键匹配使用同一表示。
  const shortcut = [
    ...MODIFIERS.filter(([field]) => event[field]).map(([, name]) => name),
    event.code,
  ].join("+");
  return isVoiceInputShortcut(shortcut) ? shortcut : null;
}

// Reads the saved chord, or returns null when none is configured or storage is unavailable.
// 读取已保存的组合键，未配置或存储不可用时返回 null。
export function readVoiceInputShortcut(): string | null {
  try {
    // Validate persisted content before it can intercept a key event.
    // 校验持久化内容后才允许其截获按键事件。
    const shortcut = window.localStorage.getItem(VOICE_INPUT_SHORTCUT_KEY);
    return shortcut && isVoiceInputShortcut(shortcut) ? shortcut : null;
  } catch {
    return null;
  }
}

// Saves a canonical chord or clears it for null. Storage errors propagate to the editor.
// 保存规范组合键，null 表示清除。存储错误交由编辑界面提示。
export function saveVoiceInputShortcut(shortcut: string | null): void {
  if (shortcut === null) window.localStorage.removeItem(VOICE_INPUT_SHORTCUT_KEY);
  else window.localStorage.setItem(VOICE_INPUT_SHORTCUT_KEY, shortcut);
}

// Formats the stored `shortcut` for display using `t` for a physical key's side.
// 格式化已保存的 `shortcut`，使用 `t` 显示物理按键所在的一侧。
export function formatVoiceInputShortcut(shortcut: string, t: Translate): string {
  if (isVoiceInputModifierKey(shortcut)) {
    const key = shortcut.replace(/(?:Left|Right)$/, "").replace(/^Control$/, "Ctrl");
    return formatMessage(t(shortcut.endsWith("Left") ? "settings.voiceInput.shortcutLeft" : "settings.voiceInput.shortcutRight"), { key });
  }
  return shortcut.split("+").map((part) => {
    if (part === "Control") return "Ctrl";
    const label = shortcutKeyLabel(part);
    return part.startsWith("Numpad") ? `Numpad ${label}` : label;
  }).join("+");
}
