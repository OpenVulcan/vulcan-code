// Validation of the language and appearance the WebUI reports for the desktop startup page.
// WebUI 为桌面启动页上报的语言与外观的校验。
//
// Kept free of Electron so it can be tested in Node; renderer reports and stored files are untrusted input.
// 不依赖 Electron，以便在 Node 中测试；渲染端上报与存储文件都是不受信输入。

import { normalizeDesktopLocale, type DesktopLocale } from './desktop-text.js';

// StartupAppearance is the language and appearance the WebUI last reported.
// StartupAppearance 是 WebUI 最近上报的语言与外观。
export interface StartupAppearance {
  // Supported language code.
  // 受支持的语言代码。
  locale: DesktopLocale;
  // Theme, text tier and motion tokens; the startup page accepts only the values it knows.
  // 主题、字号档位与动效标记；启动页只接受它认识的取值。
  theme: string;
  text: string;
  motion: string;
  // Ground color as `#rrggbb`, used as the native window background.
  // 底色 `#rrggbb`，用作原生窗口背景。
  ground: string;
}

// APPEARANCE_FILE_VERSION identifies the remembered appearance file format.
// APPEARANCE_FILE_VERSION 标识所记外观文件的格式。
export const APPEARANCE_FILE_VERSION = 1;

// APPEARANCE_TOKEN accepts the short lowercase tokens the WebUI uses for theme, text tier and motion.
// APPEARANCE_TOKEN 接受 WebUI 用于主题、字号档位与动效的小写短标记。
const APPEARANCE_TOKEN = /^[a-z][a-z0-9-]{0,31}$/;

// GROUND_COLOR accepts only an opaque hex color, the form Electron's window background takes.
// GROUND_COLOR 只接受不透明十六进制颜色，即 Electron 窗口背景接受的形式。
const GROUND_COLOR = /^#[0-9a-fA-F]{6}$/;

// parseStartupAppearance validates one reported or stored appearance; returns null when any field is unusable.
// parseStartupAppearance 校验一份上报或存储的外观；任一字段不可用时返回 null。
// Parameters: value is untrusted renderer or file data.
// 参数：value 为不受信的渲染端或文件数据。
export function parseStartupAppearance(value: unknown): StartupAppearance | null {
  if (typeof value !== 'object' || value === null) return null;
  const record = value as Record<string, unknown>;
  const locale = typeof record.locale === 'string' ? normalizeDesktopLocale(record.locale) : null;
  const tokens = [record.theme, record.text, record.motion];
  if (!locale || !tokens.every((token) => typeof token === 'string' && APPEARANCE_TOKEN.test(token))) return null;
  if (typeof record.ground !== 'string' || !GROUND_COLOR.test(record.ground)) return null;
  return {
    locale,
    theme: record.theme as string,
    text: record.text as string,
    motion: record.motion as string,
    ground: record.ground,
  };
}
