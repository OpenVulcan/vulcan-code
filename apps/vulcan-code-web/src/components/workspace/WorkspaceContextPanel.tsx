import React from "react";
import { PanelRightClose } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs";
import type {
  DiffReviewSelection,
  WorkspaceDiffFileDetailRequest,
  WorkspaceDiffFileDetailResponse,
  WorkspaceDiffSearchRequest,
  WorkspaceDiffSearchResponse,
  WorkspaceDiffSnapshotResponse,
} from "@/diff/types";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type { WorkspaceDiffFileDetailLoadOptions } from "@/service/workbench-snapshot-service";
import type {
  ConversationMessage,
  WorkbenchSnapshot,
} from "@/types";
import { WorkspaceMemoryPanel } from "@/components/workspace/WorkspaceMemoryPanel";

// LazyWorkspaceReviewPanelHost loads the heavy Git review surface only when the context rail renders it.
// LazyWorkspaceReviewPanelHost 仅在上下文栏渲染审核页时加载较重的 Git 审核界面。
const LazyWorkspaceReviewPanelHost = React.lazy(async () => {
  const module = await import("@/components/workspace/WorkspaceReviewPanelHost");
  return { default: module.WorkspaceReviewPanelHost };
});

// WorkspaceDraftConversationSelection carries a local draft conversation route.
// WorkspaceDraftConversationSelection 承载本地草稿会话路由。
export interface WorkspaceDraftConversationSelection {
  // Draft conversation id used by the selected route.
  // 当前选中路由使用的草稿会话标识。
  id: string;
  // Project id that owns the draft conversation.
  // 草稿会话所属的项目标识。
  projectId: string;
}

// WorkspaceConversationBlock is the minimal conversation block shape needed by diff extraction.
// WorkspaceConversationBlock 是 diff 提取所需的最小会话块结构。
export interface WorkspaceConversationBlock {
  // Block kind projected from a transcript message.
  // 从转录消息投影出的块类型。
  kind: string;
  // Block body text.
  // 块正文文本。
  body: string;
}

// WorkspaceContextPanelProps describes the right-side workspace context panel boundary.
// WorkspaceContextPanelProps 描述右侧工作区上下文面板边界。
export interface WorkspaceContextPanelProps {
  // Current draft conversation selection when the selected route is not persisted yet.
  // 当前路由尚未持久化时对应的草稿会话选择。
  selectedDraftConversation: WorkspaceDraftConversationSelection | null;
  // Current selected conversation id from the local workbench route.
  // 来自本地工作台路由的当前选中会话标识。
  selectedConversationId: string | null;
  // Workbench snapshot carrying selected-session side-panel projections.
  // 携带当前选中会话侧面板投影的工作台快照。
  workbench: WorkbenchSnapshot;
  // Whether the panel is rendered as a floating overlay.
  // 面板是否以浮动覆盖层方式渲染。
  floating?: boolean;
  // Closes the right context panel.
  // 关闭右侧上下文面板。
  onClose?: () => void;
  // Copies raw diff text using the host-aware clipboard implementation.
  // 使用宿主感知的剪贴板实现复制原始 diff 文本。
  onCopyRawDiff: (rawDiff: string) => void;
  // Parses one conversation message into render blocks.
  // 将单条会话消息解析为渲染块。
  parseConversationBlocks: (message: ConversationMessage) => WorkspaceConversationBlock[];
  // Loads one live workspace Git diff snapshot.
  // 加载一次实时工作区 Git diff 快照。
  loadWorkspaceDiffSnapshot: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    request: DiffReviewSelection,
  ) => Promise<WorkspaceDiffSnapshotResponse>;
  // Loads one live workspace Git diff file detail.
  // 加载一次实时工作区 Git diff 文件详情。
  loadWorkspaceDiffFileDetail: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    request: DiffReviewSelection,
    file: WorkspaceDiffFileDetailRequest,
    options?: WorkspaceDiffFileDetailLoadOptions,
  ) => Promise<WorkspaceDiffFileDetailResponse>;
  // Loads compact backend file-level search hits for one workspace diff.
  // 为单个工作区 diff 加载紧凑的后端文件级搜索命中。
  loadWorkspaceDiffSearch: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    request: DiffReviewSelection,
    search: WorkspaceDiffSearchRequest,
  ) => Promise<WorkspaceDiffSearchResponse>;
}

// WorkspaceContextPanel renders review and memory tabs for the right context rail.
// WorkspaceContextPanel 渲染右侧上下文栏中的审核与记忆标签页。
export function WorkspaceContextPanel(props: WorkspaceContextPanelProps) {
  // t translates the right context panel shell labels.
  // t 翻译右侧上下文面板外壳标签。
  const { t } = useI18n();

  return (
    <aside className={cn("context-panel-shell", props.floating && "floating")}>
      <Tabs className="context-tabs" defaultValue="review">
        <div className="context-panel-header">
          <TabsList className="context-tabs-list">
            <TabsTrigger value="review">{t("contextPanel.review")}</TabsTrigger>
            <TabsTrigger value="memory">{t("contextPanel.memory")}</TabsTrigger>
          </TabsList>
          {props.floating && props.onClose ? (
            <Button
              aria-label={t("contextPanel.close")}
              className="context-panel-close-button"
              onClick={props.onClose}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <PanelRightClose size={16} />
            </Button>
          ) : null}
        </div>
        <TabsContent className="context-tab-content review-tab-content" value="review">
          <React.Suspense fallback={<WorkspaceReviewPanelLoading />}>
            <LazyWorkspaceReviewPanelHost
              loadWorkspaceDiffFileDetail={props.loadWorkspaceDiffFileDetail}
              loadWorkspaceDiffSearch={props.loadWorkspaceDiffSearch}
              loadWorkspaceDiffSnapshot={props.loadWorkspaceDiffSnapshot}
              parseConversationBlocks={props.parseConversationBlocks}
              selectedConversationId={props.selectedConversationId}
              selectedDraftConversation={props.selectedDraftConversation}
              workbench={props.workbench}
              onCopyRawDiff={props.onCopyRawDiff}
            />
          </React.Suspense>
        </TabsContent>
        <TabsContent className="context-tab-content" value="memory">
          <WorkspaceMemoryPanel snapshot={props.workbench.contextInjection ?? null} />
        </TabsContent>
      </Tabs>
    </aside>
  );
}

// WorkspaceReviewPanelLoading renders while the review panel chunk is loading.
// WorkspaceReviewPanelLoading 渲染审核面板 chunk 加载期间的轻量状态。
function WorkspaceReviewPanelLoading() {
  return (
    <div className="context-panel-loading-state">
      <span>正在加载审核面板。</span>
    </div>
  );
}
