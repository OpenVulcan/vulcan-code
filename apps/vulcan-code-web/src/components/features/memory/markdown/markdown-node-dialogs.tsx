import * as React from "react";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Input } from "@/ui-kit/primitives/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { Textarea } from "@/ui-kit/primitives/textarea";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n-messages";
import type {
  MarkdownMemoryNode,
  MarkdownMemoryOperation,
  MarkdownMemoryOverview,
  MarkdownMemoryScope,
} from "@/types";

// SCOPE_LABELS names each scope in pickers.
// SCOPE_LABELS 为选择器中的每层命名。
export const SCOPE_LABELS: Record<MarkdownMemoryScope, TranslationKey> = {
  user: "memory.markdown.nav.user",
  space: "memory.markdown.nav.space",
  project: "memory.markdown.nav.project",
};

// SectionChoice is one destination section: an existing H1 node or a preset not created yet.
// SectionChoice 是一个目标分区：已有的 H1 节点或尚未创建的预置分区。
interface SectionChoice {
  // Picker value: `node:<id>` or `preset:<key>`.
  // 选择器值：`node:<id>` 或 `preset:<key>`。
  value: string;
  // Displayed title.
  // 显示标题。
  title: string;
}

// sectionChoices lists the sections of one scope; presets cover sections the file does not have yet.
// sectionChoices 列出某层的分区；预置项补齐文件中尚不存在的分区。
export function sectionChoices(overview: MarkdownMemoryOverview, scope: MarkdownMemoryScope): SectionChoice[] {
  const roots = overview[scope].filter((node) => node.level === 1);
  const existingKeys = new Set(roots.map((node) => node.section).filter((key): key is string => key !== null));
  return [
    ...roots.map((node) => ({ value: `node:${node.id}`, title: node.path[0] ?? "" })),
    ...overview.presetSections
      .filter((section) => section.scope === scope && !existingKeys.has(section.key))
      .map((section) => ({ value: `preset:${section.key}`, title: section.title })),
  ];
}

// placement turns a section choice into operation fields.
// placement 将分区选择转为操作字段。
function placement(value: string): { parentId?: string; section?: string } {
  if (value.startsWith("node:")) return { parentId: value.slice("node:".length) };
  return { section: value.slice("preset:".length) };
}

// NodeEditorState says which editor is open and on what.
// NodeEditorState 表示打开了哪个编辑器以及其对象。
export type NodeEditorState =
  | { kind: "add" }
  | { kind: "edit"; node: MarkdownMemoryNode }
  | { kind: "move"; node: MarkdownMemoryNode };

// NodeEditorDialog adds, edits or moves one node and submits the resulting operations.
// NodeEditorDialog 新增、编辑或移动一个节点，并提交生成的操作。
export function NodeEditorDialog(props: {
  // Open editor, or null when closed.
  // 已打开的编辑器；关闭时为 null。
  state: NodeEditorState | null;
  // Scope of the page.
  // 页面所属层。
  scope: MarkdownMemoryScope;
  // Page data used for section choices.
  // 用于分区选择的页面数据。
  overview: MarkdownMemoryOverview;
  // Whether the project scope is writable here.
  // 此处项目层是否可写。
  projectAvailable: boolean;
  // Closes the editor.
  // 关闭编辑器。
  onClose: () => void;
  // Applies operations; resolves the refreshed page, or null when nothing was written.
  // 应用操作；解析为刷新后的页面，未写入时为 null。
  onSubmit: (operations: MarkdownMemoryOperation[]) => Promise<MarkdownMemoryOverview | null>;
}) {
  const { t } = useI18n();
  const { state } = props;
  const [title, setTitle] = React.useState("");
  const [body, setBody] = React.useState("");
  const [targetScope, setTargetScope] = React.useState<MarkdownMemoryScope>(props.scope);
  const [section, setSection] = React.useState("");
  const [saving, setSaving] = React.useState(false);
  // Each opening starts from the node being edited, or empty for a new node.
  // 每次打开都从所编辑节点开始；新节点为空。
  React.useEffect(() => {
    if (!state) return;
    setTitle(state.kind === "add" ? "" : state.node.path[state.node.path.length - 1] ?? "");
    setBody(state.kind === "edit" ? state.node.body : "");
    setTargetScope(props.scope);
    setSection(sectionChoices(props.overview, props.scope)[0]?.value ?? "");
  }, [state, props.scope, props.overview]);

  const choices = sectionChoices(props.overview, targetScope);
  const scopes = (["user", "space", "project"] as const).filter((scope) => scope !== "project" || props.projectAvailable);
  const currentTitle = state && state.kind !== "add" ? state.node.path[state.node.path.length - 1] ?? "" : "";
  const titleChanged = state?.kind === "edit" && title.trim() !== currentTitle;
  const bodyChanged = state?.kind === "edit" && body !== state.node.body;
  const valid = state?.kind === "move"
    ? section !== ""
    : title.trim() !== "" && (state?.kind === "add" ? section !== "" : titleChanged || bodyChanged);
  // submit writes the change. An edit that changes both parts writes the body first and then renames
  // the refreshed node, because the revision covers the title and the body.
  // submit 写入变更。同时修改两部分的编辑先写正文，再重命名刷新后的节点，因为修订覆盖标题与正文。
  const submit = async () => {
    if (!state) return;
    setSaving(true);
    try {
      if (state.kind === "add") {
        if (await props.onSubmit([{ action: "create", scope: props.scope, title: title.trim(), body, ...placement(section) }])) props.onClose();
        return;
      }
      if (state.kind === "move") {
        if (await props.onSubmit([{ action: "move", id: state.node.id, expectedRevision: state.node.revision, scope: targetScope, ...placement(section) }])) props.onClose();
        return;
      }
      let node: MarkdownMemoryNode | undefined = state.node;
      if (bodyChanged) {
        const overview = await props.onSubmit([{ action: "replace", id: node.id, expectedRevision: node.revision, body }]);
        if (!overview) return;
        const path = JSON.stringify(node.path);
        node = overview[props.scope].find((candidate) => JSON.stringify(candidate.path) === path);
      }
      if (titleChanged && node) {
        if (!await props.onSubmit([{ action: "rename", id: node.id, expectedRevision: node.revision, title: title.trim() }])) return;
      }
      props.onClose();
    } finally {
      setSaving(false);
    }
  };
  const heading = state?.kind === "add" ? "memory.markdown.editor.addTitle"
    : state?.kind === "move" ? "memory.markdown.editor.moveTitle" : "memory.markdown.editor.editTitle";

  return <Dialog open={state !== null} onOpenChange={(open) => { if (!open && !saving) props.onClose(); }}>
    <DialogContent size="lg" showCloseButton={false} data-memory-node-editor="">
      <DialogHeader>
        <DialogTitle>{t(heading)}</DialogTitle>
        {state?.kind === "move" ? <DialogDescription>{title}</DialogDescription> : null}
      </DialogHeader>
      <div className="vk-mm-editor">
        {state?.kind === "move" ? <label className="vk-mm-field">
          <span>{t("memory.markdown.editor.scope")}</span>
          <Select value={targetScope} disabled={saving} onValueChange={(value) => {
            const scope = value as MarkdownMemoryScope;
            setTargetScope(scope);
            setSection(sectionChoices(props.overview, scope)[0]?.value ?? "");
          }}>
            <SelectTrigger className="vk-fill"><SelectValue /></SelectTrigger>
            <SelectContent>
              {scopes.map((scope) => <SelectItem key={scope} value={scope}>{t(SCOPE_LABELS[scope])}</SelectItem>)}
            </SelectContent>
          </Select>
        </label> : null}
        {state?.kind !== "edit" ? <label className="vk-mm-field">
          <span>{t("memory.markdown.editor.section")}</span>
          <Select value={section} disabled={saving || choices.length === 0} onValueChange={setSection}>
            <SelectTrigger className="vk-fill"><SelectValue /></SelectTrigger>
            <SelectContent>
              {choices
                .filter((choice) => state?.kind !== "move" || choice.value !== `node:${state.node.id}`)
                .map((choice) => <SelectItem key={choice.value} value={choice.value}>{choice.title}</SelectItem>)}
            </SelectContent>
          </Select>
        </label> : null}
        {state?.kind !== "move" ? <>
          <label className="vk-mm-field">
            <span>{t("memory.markdown.editor.title")}</span>
            <Input value={title} disabled={saving} maxLength={120} onChange={(event) => setTitle(event.target.value)} />
          </label>
          <label className="vk-mm-field">
            <span>{t("memory.markdown.editor.body")}</span>
            <Textarea density="large" rows={8} value={body} disabled={saving} onChange={(event) => setBody(event.target.value)} />
          </label>
        </> : null}
      </div>
      <DialogFooter>
        <Button type="button" variant="outline" disabled={saving} onClick={props.onClose}>{t("conversation.action.cancel")}</Button>
        <Button type="button" disabled={saving || !valid} data-memory-node-submit="" onClick={() => void submit()}>{t("memory.markdown.save")}</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}

// FileEditorDialog edits a whole memory file as the fallback for anything the node editor cannot do.
// FileEditorDialog 以整文件编辑兜底节点编辑器无法完成的操作。
export function FileEditorDialog(props: {
  // Text the editor loaded, or null when closed.
  // 编辑器加载的文本；关闭时为 null。
  file: { path: string; text: string } | null;
  // Closes the editor.
  // 关闭编辑器。
  onClose: () => void;
  // Saves the text; resolves true when written.
  // 保存文本；写入成功时解析为 true。
  onSave: (expected: string, text: string) => Promise<boolean>;
}) {
  const { t } = useI18n();
  const [text, setText] = React.useState("");
  const [saving, setSaving] = React.useState(false);
  React.useEffect(() => {
    if (props.file) setText(props.file.text);
  }, [props.file]);
  const save = async () => {
    if (!props.file) return;
    setSaving(true);
    try {
      if (await props.onSave(props.file.text, text)) props.onClose();
    } finally {
      setSaving(false);
    }
  };
  return <Dialog open={props.file !== null} onOpenChange={(open) => { if (!open && !saving) props.onClose(); }}>
    <DialogContent size="xl" scrollMode="fixed-footer" showCloseButton={false} data-memory-file-editor="">
      <DialogHeader>
        <DialogTitle>{t("memory.markdown.file.title")}</DialogTitle>
        <DialogDescription>{t("memory.markdown.file.hint")}</DialogDescription>
      </DialogHeader>
      <div className="vk-mm-editor vk-dialog-body">
        <code className="vk-mm-path">{props.file?.path}</code>
        <Textarea className="vk-mm-file-text" density="large" rows={22} spellCheck={false} value={text} disabled={saving}
          aria-label={t("memory.markdown.file.title")} onChange={(event) => setText(event.target.value)} />
      </div>
      <DialogFooter>
        <Button type="button" variant="outline" disabled={saving} onClick={props.onClose}>{t("conversation.action.cancel")}</Button>
        <Button type="button" disabled={saving || text === props.file?.text} onClick={() => void save()}>{t("memory.markdown.save")}</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}
