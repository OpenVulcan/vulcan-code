// motionTokenMs reads one motion duration token from the live stylesheet, so a timer that waits for a CSS
// transition shares the stylesheet's single duration instead of repeating it. Without the stylesheet (tests) there
// is no transition to wait for, so the result is zero.
// motionTokenMs 从实时样式表读取一个动效时长标记，使等待 CSS 过渡的计时器共用样式表中的唯一时长而不重复定义。
// 没有样式表（测试环境）时不存在需要等待的过渡，因此结果为零。
export function motionTokenMs(token: "--vk-t-fast" | "--vk-t-med"): number {
  if (typeof document === "undefined") return 0;
  const value = getComputedStyle(document.documentElement).getPropertyValue(token).trim();
  const amount = Number.parseFloat(value);
  if (!Number.isFinite(amount)) return 0;
  return value.endsWith("ms") ? amount : value.endsWith("s") ? amount * 1000 : 0;
}
