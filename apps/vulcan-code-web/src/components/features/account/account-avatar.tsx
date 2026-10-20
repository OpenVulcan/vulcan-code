import { UserRound } from "lucide-react";
import { cx } from "@/ui-kit/behavior/core";

// accountInitial returns the first user-perceived character of name, upper-cased for scripts with case,
// or null when there is no name to show.
// accountInitial 返回 name 的第一个用户可感知字符（有大小写的文字转为大写）；没有可显示的名称时返回 null。
export function accountInitial(name: string | null | undefined): string | null {
  const trimmed = name?.trim();
  if (!trimmed) return null;
  // Segmenter keeps emoji and combined characters whole; Array.from is the code-point fallback.
  // Segmenter 保持表情与组合字符完整；Array.from 是按码点的回退。
  const first = typeof Intl.Segmenter === "function"
    ? new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(trimmed)[Symbol.iterator]().next().value?.segment
    : Array.from(trimmed)[0];
  return first ? first.toLocaleUpperCase() : null;
}

// AccountAvatar shows the signed-in account's initial, or a person icon for the local user.
// AccountAvatar 显示已登录账户的首字符；本地用户显示人像图标。
export function AccountAvatar(props: {
  // Verified account name, or null when nobody is signed in.
  // 已验证的账户名称；未登录时为 null。
  name: string | null | undefined;
  // Size and placement classes from the surrounding surface.
  // 来自所在界面的尺寸与布局类。
  className?: string;
}) {
  const initial = accountInitial(props.name);
  return (
    <span aria-hidden="true" className={cx("account-avatar", props.className)} data-signed-in={initial ? "" : undefined}>
      {initial ?? <UserRound size={16} />}
    </span>
  );
}
