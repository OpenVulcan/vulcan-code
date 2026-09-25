import { Icon as IconifyIcon, addCollection } from "@iconify/react";
import { FileWarning } from "lucide-react";
import { workspaceFileTreeExtensionLabel } from "@/components/workspace/workspace-file-tree-drawer";
import { FILE_ICON_COLLECTION } from "@/generated/file-icon-collection";

addCollection(FILE_ICON_COLLECTION);

// FileTypeIconModel describes the resolved icon source for one file path.
// FileTypeIconModel 描述单个文件路径解析出的图标来源。
export interface FileTypeIconModel {
  // Iconify icon name when the extension is covered by a colored library icon.
  // 扩展名命中彩色图库时使用的 Iconify 图标名。
  iconName: string | null;
  // File name used by the fallback extension badge.
  // 回退扩展名徽标使用的文件名。
  fallbackName: string;
}

// FileTypeIcon renders the compact icon used by file change presentation rows.
// FileTypeIcon 渲染文件变更展示行使用的紧凑图标。
export function FileTypeIcon(props: {
  // Resolved icon model for the rendered file.
  // 当前文件解析后的图标模型。
  icon: FileTypeIconModel;
  // Whether the file should show a warning fallback.
  // 文件是否应展示警告回退图标。
  warning?: boolean;
}) {
  if (props.warning) {
    return <FileWarning aria-hidden="true" size={14} />;
  }
  if (props.icon.iconName) {
    return <IconifyIcon aria-hidden="true" height={16} icon={props.icon.iconName} width={16} />;
  }
  return <span aria-hidden="true" className="file-change-fallback-icon">{workspaceFileTreeExtensionLabel(props.icon.fallbackName)}</span>;
}

// resolveFileTypeIcon returns a stable file icon for common source and data extensions.
// resolveFileTypeIcon 为常见源码与数据扩展名返回稳定文件图标。
export function resolveFileTypeIcon(
  // File path or file name used to inspect the extension.
  // 用于检查扩展名的文件路径或文件名。
  path: string,
): FileTypeIconModel {
  const fileName = basenameFromPath(path);
  const extension = extensionFromName(fileName);
  const iconName = FILE_EXTENSION_ICON_NAMES.get(extension) ?? FILE_NAME_ICON_NAMES.get(fileName.toLowerCase()) ?? null;
  return {
    fallbackName: fileName || path || "file",
    iconName,
  };
}

// basenameFromPath extracts the final path segment across Windows and POSIX separators.
// basenameFromPath 跨 Windows 与 POSIX 分隔符提取最终路径片段。
function basenameFromPath(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  return normalized.split("/").filter(Boolean).at(-1) ?? path;
}

// extensionFromName extracts a lowercase extension without the dot.
// extensionFromName 提取不带点号的小写扩展名。
function extensionFromName(name: string): string {
  const index = name.lastIndexOf(".");
  if (index <= 0 || index >= name.length - 1) {
    return "";
  }
  return name.slice(index + 1).toLowerCase();
}

const FILE_NAME_ICON_NAMES = new Map<string, string>([
  ["dockerfile", "material-icon-theme:docker"],
  ["makefile", "material-icon-theme:makefile"],
  ["readme.md", "material-icon-theme:readme"],
  ["package.json", "material-icon-theme:nodejs"],
  ["tsconfig.json", "material-icon-theme:tsconfig"],
  ["vite.config.ts", "material-icon-theme:vite"],
  ["vite.config.js", "material-icon-theme:vite"],
]);

const FILE_EXTENSION_ICON_NAMES = new Map<string, string>([
  ["astro", "material-icon-theme:astro"],
  ["bat", "material-icon-theme:console"],
  ["c", "material-icon-theme:c"],
  ["cc", "material-icon-theme:cpp"],
  ["cmd", "material-icon-theme:console"],
  ["cpp", "material-icon-theme:cpp"],
  ["cs", "material-icon-theme:csharp"],
  ["css", "material-icon-theme:css"],
  ["csv", "material-icon-theme:table"],
  ["dart", "material-icon-theme:dart"],
  ["dockerfile", "material-icon-theme:docker"],
  ["env", "material-icon-theme:tune"],
  ["go", "material-icon-theme:go"],
  ["h", "material-icon-theme:c"],
  ["hpp", "material-icon-theme:cpp"],
  ["html", "material-icon-theme:html"],
  ["java", "material-icon-theme:java"],
  ["js", "material-icon-theme:javascript"],
  ["json", "material-icon-theme:json"],
  ["jsx", "material-icon-theme:react"],
  ["kt", "material-icon-theme:kotlin"],
  ["less", "material-icon-theme:less"],
  ["lock", "material-icon-theme:lock"],
  ["lua", "material-icon-theme:lua"],
  ["md", "material-icon-theme:markdown"],
  ["mdx", "material-icon-theme:mdx"],
  ["mermaid", "material-icon-theme:mermaid"],
  ["mjs", "material-icon-theme:javascript"],
  ["php", "material-icon-theme:php"],
  ["ps1", "material-icon-theme:powershell"],
  ["py", "material-icon-theme:python"],
  ["rb", "material-icon-theme:ruby"],
  ["rs", "material-icon-theme:rust"],
  ["scss", "material-icon-theme:sass"],
  ["sh", "material-icon-theme:console"],
  ["sql", "material-icon-theme:database"],
  ["svelte", "material-icon-theme:svelte"],
  ["svg", "material-icon-theme:svg"],
  ["swift", "material-icon-theme:swift"],
  ["toml", "material-icon-theme:toml"],
  ["ts", "material-icon-theme:typescript"],
  ["tsx", "material-icon-theme:react-ts"],
  ["txt", "material-icon-theme:document"],
  ["vue", "material-icon-theme:vue"],
  ["xml", "material-icon-theme:xml"],
  ["yaml", "material-icon-theme:yaml"],
  ["yml", "material-icon-theme:yaml"],
]);
