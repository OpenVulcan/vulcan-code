import * as React from "react";
import { Download, Upload } from "lucide-react";
import { PageHint, PageSection } from "@/components/page";
import { ProjectFolderPickerDialog } from "@/components/project/folder-picker-dialog";
import { PackageFilePickerDialog } from "@/components/project/package-file-picker-dialog";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Checkbox } from "@/ui-kit/primitives/checkbox";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { InlineDisclosure } from "@/ui-kit/primitives/disclosure";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { Spinner } from "@/ui-kit/primitives/spinner";
import { toast } from "@/ui-kit/primitives/toast";
import { formatMessage, useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n-messages";
import { errorMessage } from "@/lib/display-format";
import {
  browseMarkdownMemoryPackages,
  exportMarkdownMemory,
  importMarkdownMemory,
  loadMarkdownMemory,
  previewMarkdownMemoryImport,
} from "@/service/markdown-memory-service";
import { browseProjectDirectories } from "@/service/project-actions-service";
import type {
  MarkdownMemoryConflictChoice,
  MarkdownMemoryImportPreview,
  MarkdownMemoryImportReport,
  MarkdownMemoryNodeConflict,
  MarkdownMemoryNodeSide,
  MarkdownMemoryProject,
  MarkdownMemoryProjectMapping,
  MarkdownMemoryScopeMergePreview,
} from "@/types";
import { SCOPE_LABELS } from "./markdown-node-dialogs";

// CANDIDATE_REASON_KEYS names the stable reasons a local project matches a packaged one.
// CANDIDATE_REASON_KEYS 为本地项目匹配包内项目的稳定原因命名。
const CANDIDATE_REASON_KEYS: Record<string, TranslationKey> = {
  same_path: "memory.markdown.transfer.reason.samePath",
  same_first_commit: "memory.markdown.orphan.reason.sameFirstCommit",
  same_directory_name: "memory.markdown.orphan.reason.sameDirectoryName",
};

// CHOICES lists the conflict resolutions in display order.
// CHOICES 按显示顺序列出冲突解决方式。
const CHOICES: ReadonlyArray<{ label: TranslationKey; value: MarkdownMemoryConflictChoice }> = [
  { label: "memory.markdown.transfer.choice.keepLocal", value: "keep_local" },
  { label: "memory.markdown.transfer.choice.takeImported", value: "take_imported" },
  { label: "memory.markdown.transfer.choice.keepBoth", value: "keep_both" },
];

// ExportSelectionDraft is the export dialog's checked scopes.
// ExportSelectionDraft 是导出对话框中已勾选的层。
interface ExportSelectionDraft {
  // Whether the user scope is checked.
  // 是否勾选用户层。
  user: boolean;
  // Whether the space scope is checked.
  // 是否勾选空间层。
  space: boolean;
  // Checked memory projects.
  // 已勾选的记忆项目。
  projectIds: string[];
}

// OpenImport is a package whose import is being reviewed.
// OpenImport 是正在审阅导入的导出包。
interface OpenImport {
  // Exact package path.
  // 精确导出包路径。
  archivePath: string;
  // Preview without project targets, used for the project step.
  // 未指定项目去向的预览，用于项目步骤。
  preview: MarkdownMemoryImportPreview;
  // Local memory projects a packaged project may be linked to.
  // 包内项目可关联的本地记忆项目。
  localProjects: MarkdownMemoryProject[];
}

// MemoryTransferSection exports memory scopes to a package and imports packages note by note.
// MemoryTransferSection 将记忆层导出为导出包，并按笔记导入导出包。
export function MemoryTransferSection(props: {
  // Called after an import changed memory, so project lists can refresh.
  // 导入改变记忆后调用，以便刷新项目列表。
  onImported: () => void;
}) {
  const { t } = useI18n();
  const [selection, setSelection] = React.useState<ExportSelectionDraft | null>(null);
  const [choosingFolder, setChoosingFolder] = React.useState(false);
  const [pickingPackage, setPickingPackage] = React.useState(false);
  const [openImport, setOpenImport] = React.useState<OpenImport | null>(null);

  // exportTo writes the selected scopes into the chosen folder.
  // exportTo 将所选层写入所选文件夹。
  const exportTo = async (destinationDirectory: string) => {
    if (!selection) return;
    const exported = await exportMarkdownMemory({
      destinationDirectory,
      user: selection.user,
      space: selection.space,
      memoryProjectIds: selection.projectIds,
    });
    setChoosingFolder(false);
    setSelection(null);
    toast.success(t("memory.markdown.transfer.exported"), { description: exported.path });
  };
  // inspect previews a chosen package and loads the local projects it may be linked to.
  // inspect 预览所选导出包，并加载可关联的本地项目。
  const inspect = async (archivePath: string) => {
    const [preview, overview] = await Promise.all([previewMarkdownMemoryImport(archivePath, []), loadMarkdownMemory({})]);
    setOpenImport({ archivePath, preview, localProjects: overview.projects });
  };

  return <PageSection data-memory-transfer="" title={t("memory.markdown.transfer.title")}
    description={t("memory.markdown.transfer.hint")}
    actions={<>
      <Button type="button" size="sm" variant="outline" onClick={() => setSelection({ user: true, space: true, projectIds: [] })}>
        <Download size={14} />{t("memory.markdown.transfer.export")}
      </Button>
      <Button type="button" size="sm" variant="outline" onClick={() => setPickingPackage(true)}>
        <Upload size={14} />{t("memory.markdown.transfer.import")}
      </Button>
    </>}>
    {selection && !choosingFolder ? <ExportSelectionDialog selection={selection} onChange={setSelection}
      onClose={() => setSelection(null)} onContinue={() => setChoosingFolder(true)} /> : null}
    {selection && choosingFolder ? <ProjectFolderPickerDialog open title={t("memory.markdown.transfer.folderTitle")}
      confirmLabel={t("memory.markdown.transfer.exportHere")} onBrowse={browseProjectDirectories}
      onOpenChange={(open) => { if (!open) setChoosingFolder(false); }} onConfirm={exportTo} /> : null}
    {pickingPackage ? <PackageFilePickerDialog open onOpenChange={setPickingPackage} onBrowse={browseMarkdownMemoryPackages}
      onSelect={inspect} title={t("memory.markdown.transfer.pickTitle")} emptyLabel={t("memory.markdown.transfer.pickEmpty")}
      confirmLabel={t("memory.markdown.transfer.preview")} busyLabel={t("memory.markdown.transfer.previewing")} /> : null}
    {openImport ? <ImportDialog key={openImport.archivePath} open={openImport}
      onClose={() => setOpenImport(null)} onImported={props.onImported} /> : null}
  </PageSection>;
}

// ExportSelectionDialog picks the scopes and memory projects to export.
// ExportSelectionDialog 选择要导出的层与记忆项目。
function ExportSelectionDialog(props: {
  // Current selection.
  // 当前选择。
  selection: ExportSelectionDraft;
  // Replaces the selection.
  // 替换选择。
  onChange: (selection: ExportSelectionDraft) => void;
  // Closes without exporting.
  // 关闭且不导出。
  onClose: () => void;
  // Continues to the folder picker.
  // 继续到文件夹选择器。
  onContinue: () => void;
}) {
  const { t } = useI18n();
  const { selection } = props;
  const [projects, setProjects] = React.useState<MarkdownMemoryProject[] | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  React.useEffect(() => {
    let live = true;
    loadMarkdownMemory({})
      .then((overview) => { if (live) setProjects(overview.projects); })
      .catch((error: unknown) => { if (live) setLoadError(errorMessage(error)); });
    return () => { live = false; };
  }, []);
  const toggleProject = (id: string, checked: boolean) => props.onChange({
    ...selection,
    projectIds: checked ? [...selection.projectIds, id] : selection.projectIds.filter((value) => value !== id),
  });
  const empty = !selection.user && !selection.space && selection.projectIds.length === 0;
  return <Dialog open onOpenChange={(open) => { if (!open) props.onClose(); }}>
    <DialogContent size="md" scrollMode="body-scroll">
      <DialogHeader>
        <DialogTitle>{t("memory.markdown.transfer.exportTitle")}</DialogTitle>
        <DialogDescription>{t("memory.markdown.transfer.exportHint")}</DialogDescription>
      </DialogHeader>
      <div className="vk-mm-checks" data-memory-export-scopes="">
        <label><Checkbox checked={selection.user} onCheckedChange={(checked) => props.onChange({ ...selection, user: checked === true })} />
          <span>{t(SCOPE_LABELS.user)}</span></label>
        <label><Checkbox checked={selection.space} onCheckedChange={(checked) => props.onChange({ ...selection, space: checked === true })} />
          <span>{t(SCOPE_LABELS.space)}</span></label>
        {loadError ? <p className="vk-mm-error" role="alert">{loadError}</p>
          : projects === null ? <Spinner />
            : projects.map((project) => <label key={project.id}>
              <Checkbox checked={selection.projectIds.includes(project.id)} onCheckedChange={(checked) => toggleProject(project.id, checked === true)} />
              <span>{formatMessage(t("memory.markdown.transfer.projectScope"), { name: project.name })}
                {project.paths[0] ? <small className="vk-mm-path">{project.paths[0]}</small> : null}</span>
            </label>)}
      </div>
      <DialogFooter>
        <Button type="button" variant="outline" onClick={props.onClose}>{t("conversation.action.cancel")}</Button>
        <Button type="button" disabled={empty} onClick={props.onContinue}>{t("memory.markdown.transfer.chooseFolder")}</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}

// ImportDialog walks through project targets, the merge review and the report.
// ImportDialog 依次完成项目去向、合并审阅与结果报告。
function ImportDialog(props: {
  // Package under review.
  // 正在审阅的导出包。
  open: OpenImport;
  // Closes the dialog.
  // 关闭对话框。
  onClose: () => void;
  // Called after the import was applied.
  // 导入应用后调用。
  onImported: () => void;
}) {
  const { t } = useI18n();
  const { archivePath, preview: initial, localProjects } = props.open;
  const hasProjects = initial.projects.length > 0;
  // targets holds each packaged project's picker value: `link:<id>`, `create`, `ignore` or "" when unset.
  // Candidates are only ordered first as hints; every project starts unset.
  // targets 保存每个包内项目的选择值：`link:<id>`、`create`、`ignore`，未选时为 ""。候选仅排在前面作提示，
  // 每个项目初始均未选择。
  const [targets, setTargets] = React.useState<Record<string, string>>(() => Object.fromEntries(
    initial.projects.map((entry) => [entry.project.id, ""]),
  ));
  const [review, setReview] = React.useState<MarkdownMemoryImportPreview | null>(hasProjects ? null : initial);
  const [choices, setChoices] = React.useState<Record<string, MarkdownMemoryConflictChoice>>(() => hasProjects ? {} : defaultChoices(initial));
  const [report, setReport] = React.useState<MarkdownMemoryImportReport | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const mappings = (): MarkdownMemoryProjectMapping[] => initial.projects.map(({ project }) => {
    const value = targets[project.id] ?? "";
    return {
      packageProjectId: project.id,
      target: value.startsWith("link:") ? { action: "link", memoryProjectId: value.slice("link:".length) }
        : value === "create" ? { action: "create" } : { action: "ignore" },
    };
  });
  const run = async (task: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await task();
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };
  const toReview = () => run(async () => {
    const next = await previewMarkdownMemoryImport(archivePath, mappings());
    setChoices(defaultChoices(next));
    setReview(next);
  });
  const apply = () => run(async () => {
    if (!review) return;
    const resolutions = review.scopes.flatMap((scope) => scope.conflicts.map((conflict) => ({
      scope: scope.scope,
      ...(scope.packageProjectId ? { packageProjectId: scope.packageProjectId } : {}),
      nodeId: conflict.id,
      choice: choices[conflictKey(scope, conflict)] ?? "keep_local",
    })));
    setReport(await importMarkdownMemory({ archivePath, expectedSha256: review.sha256, projects: mappings(), resolutions }));
    props.onImported();
  });
  const fillUnset = (value: string) => setTargets(Object.fromEntries(Object.entries(targets).map(([id, current]) => [id, current || value])));
  const unset = Object.values(targets).some((value) => value === "");
  const projectName = (packageProjectId: string | null) =>
    initial.projects.find((entry) => entry.project.id === packageProjectId)?.project.name ?? packageProjectId ?? "";

  let body: React.ReactNode;
  let footer: React.ReactNode;
  if (report) {
    body = <div className="vk-mm-import-report" data-memory-import-report="">
      <p>{formatMessage(t("memory.markdown.transfer.report"), { added: report.added, replaced: report.replaced, unchanged: report.unchanged })}</p>
      {report.createdProjects.length > 0
        ? <p>{formatMessage(t("memory.markdown.transfer.reportCreated"), { count: report.createdProjects.length })}</p> : null}
      {report.backupDirectory
        ? <p>{t("memory.markdown.transfer.reportBackup")}<code className="vk-mm-path">{report.backupDirectory}</code></p> : null}
    </div>;
    footer = <Button type="button" onClick={props.onClose}>{t("memory.markdown.transfer.done")}</Button>;
  } else if (!review) {
    body = <div className="vk-mm-import-projects" data-memory-import-projects="">
      <PageHint>{t("memory.markdown.transfer.projectsHint")}</PageHint>
      {initial.projects.map(({ project, candidates }) => {
        const candidateIds = new Set(candidates.map((candidate) => candidate.memoryProjectId));
        return <div key={project.id} className="vk-mm-import-project">
          <div className="vk-mm-import-project-name">
            <strong>{project.name}</strong>
            {project.paths[0] ? <small className="vk-mm-path">{project.paths[0]}</small> : null}
          </div>
          <Select disabled={busy} value={targets[project.id] || undefined}
            onValueChange={(value) => setTargets({ ...targets, [project.id]: value })}>
            <SelectTrigger aria-label={formatMessage(t("memory.markdown.transfer.targetFor"), { name: project.name })} size="sm">
              <SelectValue placeholder={t("memory.markdown.transfer.targetUnset")} />
            </SelectTrigger>
            <SelectContent>
              {candidates.map((candidate) => <SelectItem key={candidate.memoryProjectId} value={`link:${candidate.memoryProjectId}`}>
                {formatMessage(t("memory.markdown.transfer.linkCandidate"), {
                  name: candidate.name,
                  reasons: candidate.reasons.map((reason) => CANDIDATE_REASON_KEYS[reason] ? t(CANDIDATE_REASON_KEYS[reason]) : reason).join(", "),
                })}
              </SelectItem>)}
              {localProjects.filter((local) => !candidateIds.has(local.id)).map((local) => <SelectItem key={local.id} value={`link:${local.id}`}>
                {formatMessage(t("memory.markdown.transfer.link"), { name: local.name })}
              </SelectItem>)}
              <SelectItem value="create">{t("memory.markdown.transfer.create")}</SelectItem>
              <SelectItem value="ignore">{t("memory.markdown.transfer.ignore")}</SelectItem>
            </SelectContent>
          </Select>
        </div>;
      })}
      {unset ? <div className="vk-mm-import-fill">
        <Button type="button" size="sm" variant="outline" disabled={busy} onClick={() => fillUnset("create")}>{t("memory.markdown.transfer.createRest")}</Button>
        <Button type="button" size="sm" variant="outline" disabled={busy} onClick={() => fillUnset("ignore")}>{t("memory.markdown.transfer.ignoreRest")}</Button>
      </div> : null}
    </div>;
    footer = <>
      <Button type="button" variant="outline" disabled={busy} onClick={props.onClose}>{t("conversation.action.cancel")}</Button>
      <Button type="button" disabled={busy || unset} onClick={() => void toReview()}>{busy ? <Spinner /> : null}{t("memory.markdown.transfer.next")}</Button>
    </>;
  } else {
    body = <div className="vk-mm-import-review" data-memory-import-review="">
      {review.scopes.length === 0 ? <PageHint>{t("memory.markdown.transfer.nothing")}</PageHint> : null}
      {review.scopes.map((scope) => <section key={`${scope.scope}:${scope.packageProjectId ?? ""}`} className="vk-mm-import-scope">
        <h3>{scope.scope === "project"
          ? formatMessage(t("memory.markdown.transfer.projectScope"), { name: projectName(scope.packageProjectId) })
          : t(SCOPE_LABELS[scope.scope])}</h3>
        <p className="vk-mm-import-counts">{formatMessage(t("memory.markdown.transfer.counts"), {
          same: scope.same, added: scope.added.length, conflicts: scope.conflicts.length,
        })}</p>
        {scope.newSections.length > 0
          ? <p className="vk-mm-import-note">{formatMessage(t("memory.markdown.transfer.newSections"), { titles: scope.newSections.join(", ") })}</p> : null}
        {scope.duplicates.length > 0
          ? <p className="vk-mm-import-note">{formatMessage(t("memory.markdown.transfer.duplicates"), {
            titles: scope.duplicates.map((node) => node.path.join(" › ")).join(", "),
          })}</p> : null}
        {scope.conflicts.map((conflict) => {
          const key = conflictKey(scope, conflict);
          return <div key={key} className="vk-mm-conflict" data-memory-conflict={conflict.id}>
            <div className="vk-mm-conflict-head">
              <strong>{conflict.path.join(" › ")}</strong>
              {conflict.newer !== "unknown" ? <Badge variant="secondary">{t(conflict.newer === "imported"
                ? "memory.markdown.transfer.newerImported" : "memory.markdown.transfer.newerLocal")}</Badge> : null}
            </div>
            <SegmentedControl aria-label={conflict.path.join(" › ")} disabled={busy} value={choices[key] ?? "keep_local"}
              onValueChange={(value) => setChoices({ ...choices, [key]: value as MarkdownMemoryConflictChoice })}>
              {CHOICES.map((choice) => <SegmentedControlItem key={choice.value} value={choice.value}>{t(choice.label)}</SegmentedControlItem>)}
            </SegmentedControl>
            <InlineDisclosure label={t("memory.markdown.transfer.compare")}>
              <div className="vk-mm-conflict-sides">
                <ConflictSide label={t("memory.markdown.transfer.local")} side={conflict.local} />
                <ConflictSide label={t("memory.markdown.transfer.imported")} side={conflict.imported} />
              </div>
            </InlineDisclosure>
          </div>;
        })}
      </section>)}
    </div>;
    footer = <>
      <Button type="button" variant="outline" disabled={busy} onClick={hasProjects ? () => setReview(null) : props.onClose}>
        {t(hasProjects ? "memory.markdown.transfer.back" : "conversation.action.cancel")}
      </Button>
      <Button type="button" disabled={busy || review.scopes.length === 0} onClick={() => void apply()}>
        {busy ? <Spinner /> : null}{t("memory.markdown.transfer.apply")}
      </Button>
    </>;
  }

  return <Dialog open onOpenChange={(open) => { if (!open && !busy) props.onClose(); }}>
    <DialogContent size="lg" scrollMode="body-scroll" showCloseButton={!busy}>
      <DialogHeader>
        <DialogTitle>{t("memory.markdown.transfer.importTitle")}</DialogTitle>
        <DialogDescription>{formatMessage(t("memory.markdown.transfer.importHint"), { time: initial.exportedAt })}</DialogDescription>
      </DialogHeader>
      <code className="vk-mm-path">{archivePath}</code>
      {body}
      {error ? <p className="vk-mm-error" role="alert">{error}</p> : null}
      <DialogFooter>{footer}</DialogFooter>
    </DialogContent>
  </Dialog>;
}

// ConflictSide shows one side of a conflicting note.
// ConflictSide 显示冲突笔记的一侧。
function ConflictSide(props: { label: string; side: MarkdownMemoryNodeSide }) {
  const { t } = useI18n();
  return <div className="vk-mm-conflict-side">
    <span>{props.label}{props.side.updated ? ` · ${formatMessage(t("memory.markdown.node.updated"), { time: props.side.updated })}` : ""}</span>
    <strong>{props.side.title}</strong>
    <pre>{props.side.body}</pre>
  </div>;
}

// conflictKey identifies one conflict across scopes.
// conflictKey 跨层标识单个冲突。
function conflictKey(scope: MarkdownMemoryScopeMergePreview, conflict: MarkdownMemoryNodeConflict): string {
  return `${scope.scope}:${scope.packageProjectId ?? ""}:${conflict.id}`;
}

// defaultChoices starts every conflict at its newer side, keeping the local note when unknown.
// defaultChoices 将每个冲突默认设为较新一侧；无法判断时保留本地笔记。
function defaultChoices(preview: MarkdownMemoryImportPreview): Record<string, MarkdownMemoryConflictChoice> {
  return Object.fromEntries(preview.scopes.flatMap((scope) => scope.conflicts.map((conflict) => [
    conflictKey(scope, conflict),
    conflict.newer === "imported" ? "take_imported" : "keep_local",
  ])));
}
