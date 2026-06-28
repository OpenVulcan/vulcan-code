import type { WorkspaceFileTreeNode } from "@/components/workspace/WorkspaceFileTreeDrawer";
import type { DiffFile } from "../types";
import { diffFilePathText, diffRevealRelativePath } from "./search";
export interface DiffFileTreeNode extends WorkspaceFileTreeNode<DiffFile> {
  // Child directories and files sorted for display.
  // 排序后用于展示的子目录与文件。
  children: DiffFileTreeNode[];
  // Aggregated added-line count for this node.
  // 当前节点聚合后的新增行数。
  additions: number;
  // Aggregated deleted-line count for this node.
  // 当前节点聚合后的删除行数。
  deletions: number;
}

// buildDiffFileTree builds a filtered directory tree and aggregates line-change counters.
// buildDiffFileTree 构建过滤后的目录树，并聚合行变更计数。
export function buildDiffFileTree(
  // Files available in the current rendered diff snapshot.
  // 当前渲染 diff 快照中的文件列表。
  files: readonly DiffFile[],
  // User-entered path filter text.
  // 用户输入的路径筛选文本。
  filter: string,
): DiffFileTreeNode {
  const root: DiffFileTreeNode = {
    path: "",
    name: "",
    children: [],
    additions: 0,
    deletions: 0,
  };
  for (const file of files) {
    if (!diffFileTreeMatchesFilter(file, filter)) {
      continue;
    }
    insertDiffFileTreeNode(root, file);
  }
  sortDiffFileTreeNode(root);
  compressDiffFileTreeDirectoryChains(root);
  return root;
}

// insertDiffFileTreeNode inserts one file and updates parent directory counters in place.
// insertDiffFileTreeNode 插入单个文件，并原地更新父目录计数。
function insertDiffFileTreeNode(
  // Mutable tree root or directory node.
  // 可变树根节点或目录节点。
  root: DiffFileTreeNode,
  // File to insert into the tree.
  // 需要插入目录树的文件。
  file: DiffFile,
): void {
  const parts = diffFileTreePathParts(file);
  let current = root;
  current.additions += file.additions;
  current.deletions += file.deletions;
  for (let index = 0; index < parts.length; index += 1) {
    const part = parts[index];
    const isFile = index === parts.length - 1;
    const path = parts.slice(0, index + 1).join("/");
    let child = current.children.find((node) => node.path === path && Boolean(node.file) === isFile);
    if (!child) {
      child = {
        path,
        name: part,
        children: [],
        file: isFile ? file : undefined,
        additions: 0,
        deletions: 0,
      };
      current.children.push(child);
    }
    child.additions += file.additions;
    child.deletions += file.deletions;
    current = child;
  }
}

// diffFileTreePathParts returns stable visible path segments for one file node.
// diffFileTreePathParts 返回单个文件节点稳定可见的路径片段。
function diffFileTreePathParts(
  // File whose best visible path should be split.
  // 需要拆分最佳可见路径的文件。
  file: DiffFile,
): string[] {
  const relativePath = diffRevealRelativePath(file) ?? file.key;
  const parts = relativePath.split(/[\\/]+/).map((part) => part.trim()).filter(Boolean);
  return parts.length ? parts : [file.key || "unknown"];
}

// diffFileTreeMatchesFilter checks whether one file should remain visible under the filter.
// diffFileTreeMatchesFilter 检查单个文件在筛选条件下是否应保持可见。
function diffFileTreeMatchesFilter(
  // File to test against the filter.
  // 需要与筛选文本匹配的文件。
  file: DiffFile,
  // User-entered path filter text.
  // 用户输入的路径筛选文本。
  filter: string,
): boolean {
  const normalizedFilter = filter.trim().toLowerCase();
  return !normalizedFilter || diffFilePathText(file).includes(normalizedFilter);
}

// sortDiffFileTreeNode sorts directories before files and names alphabetically.
// sortDiffFileTreeNode 将目录排在文件前，并按名称排序。
function sortDiffFileTreeNode(
  // Directory node whose descendants should be sorted.
  // 需要排序其后代的目录节点。
  node: DiffFileTreeNode,
): void {
  node.children.sort(compareDiffFileTreeNodes);
  for (const child of node.children) {
    if (!child.file) {
      sortDiffFileTreeNode(child);
    }
  }
}

// compressDiffFileTreeDirectoryChains merges single-child directory chains into compact path rows.
// compressDiffFileTreeDirectoryChains 将单子目录链合并为紧凑路径行。
function compressDiffFileTreeDirectoryChains(
  // Mutable root node whose directory descendants should be compacted.
  // 需要压缩其目录后代的可变根节点。
  root: DiffFileTreeNode,
): void {
  root.children = root.children.map(compressDiffFileTreeDirectoryNode);
}

// compressDiffFileTreeDirectoryNode returns one node with folder-only chains merged.
// compressDiffFileTreeDirectoryNode 返回已合并纯目录链的单个节点。
function compressDiffFileTreeDirectoryNode(
  // Tree node to compact.
  // 需要压缩的目录树节点。
  node: DiffFileTreeNode,
): DiffFileTreeNode {
  if (node.file) {
    return node;
  }
  let compacted: DiffFileTreeNode = {
    ...node,
    children: node.children.map(compressDiffFileTreeDirectoryNode),
  };
  while (compacted.children.length === 1 && !compacted.children[0].file) {
    const child = compacted.children[0];
    compacted = {
      ...child,
      name: `${compacted.name}/${child.name}`,
    };
  }
  return compacted;
}

// compareDiffFileTreeNodes compares tree nodes for directory-first display.
// compareDiffFileTreeNodes 按目录优先的展示方式比较树节点。
function compareDiffFileTreeNodes(
  // Left tree node.
  // 左侧树节点。
  left: DiffFileTreeNode,
  // Right tree node.
  // 右侧树节点。
  right: DiffFileTreeNode,
): number {
  if (Boolean(left.file) !== Boolean(right.file)) {
    return left.file ? 1 : -1;
  }
  return left.name.localeCompare(right.name, undefined, { numeric: true, sensitivity: "base" });
}

