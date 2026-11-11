import React from "react";
import { AtSign, Copy, FolderOpen } from "lucide-react";
import { toast } from "@/ui-kit/primitives/toast";
import { requestComposerCommand } from "@/components/composer/composer-external-commands";
import { pathMentionText } from "@/components/composer/composer-panel/use-composer-path-mention";
import { openProjectDirectoryPath } from "@/service/project-actions-service";
import type { TranslationKey } from "@/i18n";

// WorkspacePathAction is one entry of a file or folder action menu.
// WorkspacePathAction 是文件或目录操作菜单中的一项。
export interface WorkspacePathAction {
  // Stable React key.
  // 稳定的 React 键。
  key: string;
  // Leading icon.
  // 前置图标。
  icon: React.ReactNode;
  // Visible label.
  // 可见标签。
  label: string;
  // Whether the entry is shown but unavailable.
  // 该项是否显示但不可用。
  disabled?: boolean;
  // Performs the action.
  // 执行该操作。
  run: () => void;
}

// WorkspacePathTarget names the file or folder the actions apply to.
// WorkspacePathTarget 指明操作所针对的文件或目录。
export interface WorkspacePathTarget {
  // Absolute host path.
  // 宿主绝对路径。
  path: string;
  // Path relative to the project root, as mentions and the relative copy use it.
  // 相对项目根目录的路径，供提及与相对路径复制使用。
  relativePath: string;
  // Whether the target is a folder.
  // 目标是否为目录。
  isDirectory: boolean;
}

// workspacePathActions returns the actions every file or folder offers: mention it in the composer, copy its absolute
// or relative path, and show it in the system file manager. Failures surface as a toast instead of passing silently.
// workspacePathActions 返回每个文件或目录都提供的操作：在输入区引用、复制绝对或相对路径、在系统文件管理器中显示。
// 失败时以提示呈现，而非静默忽略。
export function workspacePathActions(
  target: WorkspacePathTarget,
  onCopyText: (text: string) => void | Promise<void>,
  t: (key: TranslationKey) => string,
): WorkspacePathAction[] {
  const report = (error: unknown) => toast.error(error instanceof Error ? error.message : String(error));
  const copy = (text: string) => void Promise.resolve().then(() => onCopyText(text)).catch(report);
  return [
    { key: "mention", icon: <AtSign size={14} />, label: t("workspaceFiles.mentionInMessage"),
      run: () => requestComposerCommand({ kind: "insert", text: pathMentionText(target.relativePath, target.isDirectory) }) },
    { key: "path", icon: <Copy size={14} />, label: t("workspaceFiles.copyPath"), run: () => copy(target.path) },
    { key: "relative", icon: <Copy size={14} />, label: t("workspaceFiles.copyRelativePath"), run: () => copy(target.relativePath) },
    { key: "reveal", icon: <FolderOpen size={14} />, label: t("workspaceFiles.revealInFolder"),
      run: () => void openProjectDirectoryPath(target.path).catch(report) },
  ];
}

// relativeToWorkspace returns `path` relative to `workspacePath`, or null when it lies outside the project.
// Windows paths compare without case and with either separator.
// relativeToWorkspace 返回 `path` 相对 `workspacePath` 的路径；位于项目之外时返回 null。
// Windows 路径比较时忽略大小写并接受两种分隔符。
export function relativeToWorkspace(workspacePath: string | null, path: string): string | null {
  if (!workspacePath) return null;
  const normalize = (value: string) => value.replace(/\\/g, "/").replace(/\/+$/, "");
  const root = normalize(workspacePath);
  const target = normalize(path);
  const windows = /^[a-z]:\//i.test(root) || root.startsWith("//");
  const same = (a: string, b: string) => windows ? a.toLowerCase() === b.toLowerCase() : a === b;
  if (!same(target.slice(0, root.length), root) || target.charAt(root.length) !== "/") return null;
  return target.slice(root.length + 1);
}
