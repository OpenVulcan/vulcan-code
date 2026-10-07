import type { ProjectIconSelection } from "@/components/project/project-icons";
import type { WorkbenchSnapshot } from "@/types";

// applyPreviewProjectIcon simulates project icon persistence for browser-only preview mode.
// applyPreviewProjectIcon 为纯浏览器预览模式模拟项目图标持久化。
export function applyPreviewProjectIcon(
  workbench: WorkbenchSnapshot,
  projectId: string,
  icon: ProjectIconSelection | null,
): { snapshot: WorkbenchSnapshot } {
  return {
    snapshot: {
      ...workbench,
      projects: workbench.projects.map((candidate) => candidate.id === projectId ? {
        ...candidate,
        iconPack: icon?.pack ?? null,
        iconKey: icon?.key ?? null,
      } : candidate),
    },
  };
}

