import * as React from "react";
import { FilePenLine, MoveRight, Pencil, Plus, Trash2 } from "lucide-react";
import { PageEmptyState, PageHint, PageNotice, PageSection } from "@/components/page";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Progress } from "@/ui-kit/primitives/progress";
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { toast } from "@/ui-kit/primitives/toast";
import { formatMessage, useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n-messages";
import { errorMessage, formatDateTime } from "@/lib/display-format";
import {
  applyMarkdownMemoryProjectAction,
  loadMarkdownMemory,
  readMarkdownMemoryFile,
  replaceMarkdownMemoryFile,
  writeMarkdownMemory,
} from "@/service/markdown-memory-service";
import type {
  MarkdownMemoryFile,
  MarkdownMemoryNode,
  MarkdownMemoryOperation,
  MarkdownMemoryOverview,
  MarkdownMemoryProjectAction,
  MarkdownMemoryScope,
  MarkdownMemoryTarget,
  MemoryBudgetPart,
  WorkbenchSnapshot,
} from "@/types";
import { BUDGET_PARTS } from "./memory-mode-page";
import { FileEditorDialog, NodeEditorDialog, type NodeEditorState } from "./markdown-node-dialogs";

// SCOPE_PARTS lists the budget parts each scope page reports.
// SCOPE_PARTS 列出每层页面报告的预算分项。
const SCOPE_PARTS: Record<MarkdownMemoryScope, MemoryBudgetPart[]> = {
  user: ["user"],
  space: ["spaceBase", "spaceCatalog"],
  project: ["projectBase", "projectCatalog"],
};

// ORPHAN_REASON_KEYS names the stable orphan reasons.
// ORPHAN_REASON_KEYS 为稳定的孤立原因命名。
const ORPHAN_REASON_KEYS: Record<string, TranslationKey> = {
  same_directory_name: "memory.markdown.orphan.reason.sameDirectoryName",
  same_first_commit: "memory.markdown.orphan.reason.sameFirstCommit",
};

// ProjectSelection is the project tab's choice: a host project or a memory project without one.
// ProjectSelection 是项目页签的选择：宿主项目，或没有宿主项目的记忆项目。
type ProjectSelection = { kind: "host"; id: string } | { kind: "memory"; id: string };

// MarkdownScopePage shows and edits one memory scope.
// MarkdownScopePage 显示并编辑一个记忆层。
export function MarkdownScopePage(props: { scope: MarkdownMemoryScope; workbench: WorkbenchSnapshot }) {
  const { t, locale } = useI18n();
  const { scope, workbench } = props;
  const defaultHost = React.useMemo(() => {
    const conversation = workbench.conversations.find((item) => item.id === workbench.selectedConversationId);
    const projectId = conversation?.projectId ?? null;
    return workbench.projects.find((project) => project.id === projectId)?.id ?? workbench.projects[0]?.id ?? null;
  }, [workbench.conversations, workbench.projects, workbench.selectedConversationId]);
  const [selection, setSelection] = React.useState<ProjectSelection | null>(defaultHost ? { kind: "host", id: defaultHost } : null);
  const target: MarkdownMemoryTarget = scope !== "project" || selection === null ? {}
    : selection.kind === "host" ? { projectId: selection.id } : { memoryProjectId: selection.id };
  const targetKey = JSON.stringify(target);
  const [overview, setOverview] = React.useState<MarkdownMemoryOverview | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [editor, setEditor] = React.useState<NodeEditorState | null>(null);
  const [deleting, setDeleting] = React.useState<MarkdownMemoryNode | null>(null);
  const [file, setFile] = React.useState<MarkdownMemoryFile | null>(null);

  const load = React.useCallback(async () => {
    setLoadError(null);
    try {
      setOverview(await loadMarkdownMemory(JSON.parse(targetKey) as MarkdownMemoryTarget));
    } catch (error) {
      setLoadError(errorMessage(error));
    }
  }, [targetKey]);
  React.useEffect(() => {
    setOverview(null);
    void load();
  }, [load]);

  // write applies operations as the user and keeps the refreshed page.
  // write 以用户身份应用操作并保留刷新后的页面。
  const write = async (operations: MarkdownMemoryOperation[]): Promise<MarkdownMemoryOverview | null> => {
    setBusy(true);
    try {
      const next = await writeMarkdownMemory(target, operations);
      setOverview(next);
      return next;
    } catch (error) {
      toast.error(t("memory.markdown.writeFailed"), { description: errorMessage(error) });
      return null;
    } finally {
      setBusy(false);
    }
  };
  // projectAction changes the registry, then reloads the page.
  // projectAction 修改注册表后重新加载页面。
  const projectAction = async (action: MarkdownMemoryProjectAction) => {
    setBusy(true);
    try {
      await applyMarkdownMemoryProjectAction(action);
      await load();
    } catch (error) {
      toast.error(t("memory.markdown.writeFailed"), { description: errorMessage(error) });
    } finally {
      setBusy(false);
    }
  };
  const shownProjectId = overview?.memoryProjectId ?? null;
  const openFile = async () => {
    try {
      setFile(await readMarkdownMemoryFile(scope, scope === "project" ? shownProjectId : null));
    } catch (error) {
      toast.error(t("memory.markdown.loadFailed"), { description: errorMessage(error) });
    }
  };
  const saveFile = async (expected: string, text: string): Promise<boolean> => {
    try {
      await replaceMarkdownMemoryFile({ scope, memoryProjectId: scope === "project" ? shownProjectId : null, expected, text });
      toast.success(t("memory.markdown.file.saved"));
      await load();
      return true;
    } catch (error) {
      toast.error(t("memory.markdown.writeFailed"), { description: errorMessage(error) });
      return false;
    }
  };

  if (scope === "project" && selection === null && workbench.projects.length === 0 && !overview?.projects.length) {
    return <PageSection title={t("memory.markdown.nav.project")}>
      <PageEmptyState title={t("memory.markdown.project.none")} />
    </PageSection>;
  }
  if (loadError) {
    return <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}
      actions={<Button type="button" size="sm" variant="outline" onClick={() => void load()}>{t("composer.capability.retry")}</Button>}>
      {t("memory.markdown.loadFailed")}
    </PageNotice>;
  }
  if (!overview) return <StreamPlaceholder label={t("memory.markdown.loading")} shape="rows" count={4} />;

  const nodes = overview[scope];
  const shownProject = overview.projects.find((project) => project.id === shownProjectId) ?? null;
  // Writing the project scope needs a bound or explicitly chosen memory project, or a host project to bind.
  // 写入项目层需要已绑定或显式选择的记忆项目，或可供绑定的宿主项目。
  const projectAvailable = shownProjectId !== null || selection?.kind === "host";
  const canWrite = scope !== "project" || projectAvailable;
  const hostIds = new Set(workbench.projects.map((project) => project.id));
  const unhostedProjects = overview.projects.filter((project) => !project.hostProjectIds.some((id) => hostIds.has(id)));
  const orphan = scope === "project" && selection?.kind === "host" ? overview.orphanCandidates[0] ?? null : null;
  const orphanProject = orphan ? overview.projects.find((project) => project.id === orphan.memoryProjectId) ?? null : null;
  const usage = overview.usage.filter((row) => SCOPE_PARTS[scope].includes(row.part));
  const warningPercent = overview.settings.warningPercent;

  return <>
    {scope === "project" ? <div className="vk-mm-toolbar">
      <Select value={selection ? `${selection.kind}:${selection.id}` : ""} disabled={busy} onValueChange={(value) => {
        const [kind, ...rest] = value.split(":");
        setSelection({ kind: kind === "memory" ? "memory" : "host", id: rest.join(":") });
      }}>
        <SelectTrigger aria-label={t("memory.markdown.project.select")} className="vk-mm-project-select" size="sm"><SelectValue /></SelectTrigger>
        <SelectContent>
          {workbench.projects.map((project) => <SelectItem key={project.id} value={`host:${project.id}`}>{project.name}</SelectItem>)}
          {unhostedProjects.length ? <SelectGroup>
            <SelectLabel>{t("memory.markdown.project.otherGroup")}</SelectLabel>
            {unhostedProjects.map((project) => <SelectItem key={project.id} value={`memory:${project.id}`}>{project.name}</SelectItem>)}
          </SelectGroup> : null}
        </SelectContent>
      </Select>
    </div> : null}

    {orphan && orphanProject && selection?.kind === "host" ? <PageNotice tone="info" data-memory-orphan={orphan.memoryProjectId}
      title={t("memory.markdown.orphan.title")}
      actions={<>
        <Button type="button" size="sm" disabled={busy}
          onClick={() => void projectAction({ action: "link", projectId: selection.id, memoryProjectId: orphan.memoryProjectId })}>
          {t("memory.markdown.orphan.link")}</Button>
        <Button type="button" size="sm" variant="outline" disabled={busy}
          onClick={() => void projectAction({ action: "create", projectId: selection.id })}>{t("memory.markdown.orphan.create")}</Button>
        <Button type="button" size="sm" variant="ghost" disabled={busy}
          onClick={() => void projectAction({ action: "ignore", projectId: selection.id })}>{t("memory.markdown.orphan.ignore")}</Button>
      </>}>
      {formatMessage(t("memory.markdown.orphan.body"), {
        name: orphanProject.name,
        reasons: orphan.reasons.map((reason) => ORPHAN_REASON_KEYS[reason] ? t(ORPHAN_REASON_KEYS[reason]) : reason).join(", "),
      })}
    </PageNotice> : null}
    {scope === "project" && shownProject && !shownProject.memoryEnabled
      ? <PageNotice tone="warning">{t("memory.markdown.project.disabled")}</PageNotice> : null}

    {usage.length ? <PageSection title={t("memory.markdown.usage.title")} data-memory-usage="">
      <div className="vk-mm-usage">
        {usage.map((row) => {
          const percent = row.budget > 0 ? Math.round((row.tokens * 100) / row.budget) : 0;
          const tone = row.tokens > row.budget ? "bad" : percent >= warningPercent ? "warn" : "ok";
          const label = BUDGET_PARTS.find((part) => part.part === row.part)?.label;
          return <div className="vk-mm-usage-row" data-tone={tone} key={row.part}>
            <div className="vk-mm-usage-head">
              <span>{label ? t(label) : row.part}</span>
              <span>{formatMessage(t("memory.markdown.usage.value"), { tokens: row.tokens, budget: row.budget })}</span>
              {tone === "bad" ? <Badge variant="destructive">{t("memory.markdown.usage.over")}</Badge>
                : tone === "warn" ? <Badge variant="warning">{t("memory.markdown.usage.near")}</Badge> : null}
            </div>
            <Progress className="vk-mm-usage-bar" aria-label={label ? t(label) : row.part} max={Math.max(row.budget, row.tokens, 1)} value={row.tokens} />
            {row.omitted.length ? <PageHint>{formatMessage(t("memory.markdown.usage.omitted"), { titles: row.omitted.join(", ") })}</PageHint> : null}
          </div>;
        })}
        {usage.some((row) => row.tokens > row.budget || (row.budget > 0 && row.tokens * 100 >= row.budget * warningPercent))
          ? <PageHint>{t("memory.markdown.usage.curateHint")}</PageHint> : null}
      </div>
    </PageSection> : null}

    <PageSection title={t(scope === "user" ? "memory.markdown.nav.user" : scope === "space" ? "memory.markdown.nav.space" : "memory.markdown.nav.project")}
      description={scope === "project" && projectAvailable && shownProjectId === null ? t("memory.markdown.project.unbound") : undefined}
      data-memory-scope={scope}
      actions={<>
        <Button type="button" size="sm" variant="outline" disabled={busy || !canWrite} data-memory-add=""
          onClick={() => setEditor({ kind: "add" })}><Plus size={14} />{t("memory.markdown.node.add")}</Button>
        <Button type="button" size="sm" variant="ghost" disabled={busy || (scope === "project" && shownProjectId === null)}
          onClick={() => void openFile()}><FilePenLine size={14} />{t("memory.markdown.file.edit")}</Button>
      </>}>
      {nodes.length === 0 ? <PageEmptyState title={t("memory.markdown.nodes.empty")} /> : <ol className="vk-mm-tree">
        {nodes.map((node) => <MemoryNodeRow key={`${node.id}:${node.revision}`} node={node} locale={locale} busy={busy || !canWrite}
          onEdit={() => setEditor({ kind: "edit", node })}
          onMove={() => setEditor({ kind: "move", node })}
          onDelete={() => setDeleting(node)} />)}
      </ol>}
    </PageSection>

    <NodeEditorDialog state={editor} scope={scope} overview={overview} projectAvailable={projectAvailable}
      onClose={() => setEditor(null)} onSubmit={write} />
    <FileEditorDialog file={file} onClose={() => setFile(null)} onSave={saveFile} />
    <AlertDialog open={deleting !== null} onOpenChange={(open) => { if (!open) setDeleting(null); }}>
      <AlertDialogContent size="md">
        <AlertDialogHeader>
          <AlertDialogTitle>{deleting ? formatMessage(t("memory.markdown.node.deleteConfirm"), { title: nodeTitle(deleting) }) : null}</AlertDialogTitle>
          {deleting && deleting.descendants > 0 ? <AlertDialogDescription>
            {formatMessage(t("memory.markdown.node.deleteChildren"), { count: deleting.descendants })}
          </AlertDialogDescription> : null}
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("conversation.action.cancel")}</AlertDialogCancel>
          <AlertDialogAction variant="danger" onClick={() => {
            const node = deleting;
            setDeleting(null);
            if (node) void write([{ action: "delete", id: node.id, expectedRevision: node.revision, withChildren: node.descendants > 0 }]);
          }}>{t("memory.markdown.node.delete")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </>;
}

// MemoryNodeRow renders one node with its metadata and actions.
// MemoryNodeRow 渲染一个节点及其元数据与操作。
function MemoryNodeRow(props: {
  node: MarkdownMemoryNode;
  locale: string;
  busy: boolean;
  onEdit: () => void;
  onMove: () => void;
  onDelete: () => void;
}) {
  const { t } = useI18n();
  const { node } = props;
  const updated = node.updated ? formatDateTime(new Date(node.updated), props.locale) : null;
  // The base section anchors the injected part and cannot be moved or deleted.
  // 基础分区承载注入部分，不能移动或删除。
  const fixed = node.level === 1 && node.section === "base";
  return <li className="vk-mm-node" data-level={node.level} data-memory-node={node.id}>
    <div className="vk-mm-node-head">
      <strong>{nodeTitle(node)}</strong>
      {node.level === 1 && node.section === "base" ? <Badge variant="outline">{t("memory.markdown.node.base")}</Badge> : null}
      {/* Sections are containers, preset ones included, so only notes carry an author badge. */}
      {/* 分区（含预置分区）只是容器，因此只有笔记显示作者徽章。 */}
      {node.source && node.level > 1 ? <Badge variant="ghost" title={node.session ?? undefined}>
        {t(node.source === "user" ? "memory.markdown.node.sourceUser" : "memory.markdown.node.sourceAi")}</Badge> : null}
      {updated ? <small>{formatMessage(t("memory.markdown.node.updated"), { time: updated })}</small> : null}
      <span className="vk-mm-node-acts">
        <Button type="button" size="icon-xs" variant="ghost" disabled={props.busy} aria-label={t("memory.markdown.node.edit")}
          title={t("memory.markdown.node.edit")} onClick={props.onEdit}><Pencil size={13} /></Button>
        {fixed ? null : <>
          <Button type="button" size="icon-xs" variant="ghost" disabled={props.busy} aria-label={t("memory.markdown.node.move")}
            title={t("memory.markdown.node.move")} onClick={props.onMove}><MoveRight size={13} /></Button>
          <Button type="button" size="icon-xs" variant="ghost" disabled={props.busy} aria-label={t("memory.markdown.node.delete")}
            title={t("memory.markdown.node.delete")} onClick={props.onDelete}><Trash2 size={13} /></Button>
        </>}
      </span>
    </div>
    {node.body ? <p className="vk-mm-node-body">{node.body}</p> : null}
  </li>;
}

// nodeTitle returns the node's own title.
// nodeTitle 返回节点自身标题。
function nodeTitle(node: MarkdownMemoryNode): string {
  return node.path[node.path.length - 1] ?? "";
}
