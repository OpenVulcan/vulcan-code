import type { TranslationKey } from "@/i18n";
import type { OutsideWorkspaceFileAccess } from "@/types";

// OUTSIDE_FILE_ACCESS_OPTIONS lists each outside-file access choice with its label, in the order controls show them.
// OUTSIDE_FILE_ACCESS_OPTIONS 按控件显示顺序列出各工作区外文件访问方式及其标签。
export const OUTSIDE_FILE_ACCESS_OPTIONS: ReadonlyArray<{ value: OutsideWorkspaceFileAccess; labelKey: TranslationKey }> = [
  { value: "deny", labelKey: "settings.outsideFiles.deny" },
  { value: "prompt", labelKey: "settings.outsideFiles.prompt" },
  { value: "allow", labelKey: "settings.outsideFiles.allow" },
];
