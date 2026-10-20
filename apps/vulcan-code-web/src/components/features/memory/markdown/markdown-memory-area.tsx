import "@/ui-kit/styles/features/markdown-memory.css";
import type { MarkdownMemoryScope, WorkbenchSnapshot } from "@/types";
import { MemoryModePage } from "./memory-mode-page";
import { MarkdownScopePage } from "./markdown-scope-page";
import { MEMORY_MODE_SECTION } from "./memory-mode-store";

// SECTION_SCOPES binds each Markdown section to its scope.
// SECTION_SCOPES 将每个 Markdown 分区绑定到其层。
const SECTION_SCOPES: Record<string, MarkdownMemoryScope> = {
  md_user: "user",
  md_space: "space",
  md_project: "project",
};

// MarkdownMemoryArea renders the memory mode settings or one Markdown memory scope.
// MarkdownMemoryArea 渲染记忆模式设置或单个 Markdown 记忆层。
export function MarkdownMemoryArea(props: {
  // Selected section: the mode settings or a Markdown scope.
  // 选中的分区：模式设置或某个 Markdown 层。
  section: string;
  // Workbench snapshot that supplies host projects and the selected conversation.
  // 提供宿主项目与当前会话的工作台快照。
  workbench: WorkbenchSnapshot;
}) {
  const scope = SECTION_SCOPES[props.section];
  return <div className="vk-mm-page" data-memory-section={props.section}>
    {props.section === MEMORY_MODE_SECTION || !scope
      ? <MemoryModePage />
      : <MarkdownScopePage key={scope} scope={scope} workbench={props.workbench} />}
  </div>;
}
