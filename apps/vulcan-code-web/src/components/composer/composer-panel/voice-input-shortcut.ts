// The shortcut belongs to the current GUI profile, like the selected microphone.
// 快捷键与麦克风选择一样，保存在当前图形客户端。
const VOICE_INPUT_SHORTCUT_KEY = "vulcan.gui.voice-input-shortcut.v1";

// Use physical key codes so releasing a key still matches after the modifiers change.
// 使用物理键码，确保修饰键状态变化后仍能识别主键的释放。
const SHORTCUT_KEY = /^(?:Key[A-Z]|Digit[0-9]|Space|F(?:[1-9]|1[0-9]|2[0-4]))$/;

// These are the canonical modifier names and their corresponding keyboard-event fields.
// 这些是规范的修饰键名称及对应的键盘事件字段。
const MODIFIERS = [
  ["ctrlKey", "Control"],
  ["altKey", "Alt"],
  ["shiftKey", "Shift"],
  ["metaKey", "Meta"],
] as const;

// Validates the stored chord, keeping ordinary letters, digits, and spaces available for typing.
// 校验保存的组合键，保留普通字母、数字与空格用于输入。
function isVoiceInputShortcut(shortcut: string): boolean {
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
    && SHORTCUT_KEY.test(code)
    && parts.join("+") === ordered.join("+")
    && (code.startsWith("F") || ordered.some((name) => name !== "Shift"));
}

// Returns the canonical chord for a supported key event, or null if the event cannot bind a shortcut.
// 返回键盘事件对应的规范组合键；事件不可绑定时返回 null。
export function voiceInputShortcutFromKey(event: KeyboardEvent): string | null {
  if (event.isComposing || event.getModifierState("AltGraph")) return null;
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

// Formats a canonical chord for display without changing its matching representation.
// 将规范组合键格式化为显示文本，保持匹配表示不变。
export function formatVoiceInputShortcut(shortcut: string): string {
  return shortcut.replace(/^Control\+/, "Ctrl+").replace(/\b(?:Key|Digit)(?=[A-Z0-9]$)/, "");
}
