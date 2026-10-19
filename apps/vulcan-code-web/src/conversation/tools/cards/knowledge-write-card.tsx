// Knowledge write card. The head names the change and the document it touched; the body shows what was written:
// the new document, the replaced passages, the move, or the files brought in.
// 知识库写入卡。标题说明变更及其涉及的文档；正文展示写入的内容：新文档、被替换的段落、移动或导入的文件。
import {
  FilePen, FilePlus2, FolderInput, FolderPlus, FolderX, Import, Trash2, type LucideIcon,
} from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { listOf, numberOf, objectOf, textOf } from "../tool-call";
import { PlainDiffView } from "../views/diff-view";
import { ErrorNote, MarkdownResult } from "../views/result-views";

// KNOWLEDGE_ACTIONS names each knowledge_write action.
// KNOWLEDGE_ACTIONS 命名每个 knowledge_write 动作。
const KNOWLEDGE_ACTIONS: Readonly<Record<string, { icon: LucideIcon; verb: TranslationKey }>> = {
  create: { icon: FilePlus2, verb: "toolCard.kb.create" },
  update: { icon: FilePen, verb: "toolCard.kb.update" },
  edit: { icon: FilePen, verb: "toolCard.kb.edit" },
  move: { icon: FolderInput, verb: "toolCard.kb.move" },
  delete: { icon: Trash2, verb: "toolCard.kb.delete" },
  folder: { icon: FolderPlus, verb: "toolCard.kb.folder" },
  delete_folder: { icon: FolderX, verb: "toolCard.kb.deleteFolder" },
  ingest: { icon: Import, verb: "toolCard.kb.ingest" },
};

// KnowledgeEdit is one exact passage replacement.
// KnowledgeEdit 是一次精确段落替换。
export interface KnowledgeEdit {
  // Replaced text.
  // 被替换的文本。
  oldText: string;
  // Replacement text.
  // 替换后的文本。
  newText: string;
}

// knowledgeEdits reads the passage replacements, dropping malformed entries.
// knowledgeEdits 读取段落替换，丢弃格式错误的条目。
export function knowledgeEdits(value: unknown): KnowledgeEdit[] {
  return listOf(value).flatMap((entry) => {
    const edit = objectOf(entry);
    const oldText = textOf(edit?.oldText);
    const newText = textOf(edit?.newText);
    return oldText !== null && newText !== null ? [{ newText, oldText }] : [];
  });
}

/** Renders one knowledge_write call.
 * 渲染一次 knowledge_write 调用。 */
export function KnowledgeWriteCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const action = textOf(call.input.action) ?? "";
  const spec = Object.hasOwn(KNOWLEDGE_ACTIONS, action) ? KNOWLEDGE_ACTIONS[action] : KNOWLEDGE_ACTIONS.update;
  const receipt = call.host?.kind === "knowledge_write_v1" ? call.host.payload : null;
  const documents = listOf(receipt?.documents).map((document) => objectOf(document) ?? {});
  const path = textOf(call.input.path) ?? textOf(documents[0]?.path) ?? textOf(call.input.documentId) ?? "";
  const sources = listOf(call.input.sources).filter((source): source is string => typeof source === "string");
  const target = action === "move"
    ? `${path} → ${textOf(call.input.newPath) ?? ""}`
    : action === "ingest" ? sources.join(" · ") : path;
  const revision = documents.length === 1 ? numberOf(documents[0].revision) : null;
  const body = textOf(call.input.body) ?? "";
  const edits = knowledgeEdits(call.input.edits);
  const description = textOf(call.input.description) ?? "";
  return (
    <ToolFrame
      {...props}
      icon={spec.icon}
      meta={(
        <>
          {documents.length > 1 ? <span>{t("toolCard.kb.documents").replace("{count}", String(documents.length))}</span> : null}
          {revision !== null ? <span>{t("toolCard.kb.revision").replace("{revision}", String(revision))}</span> : null}
        </>
      )}
      state={call.state}
      target={target}
      targetTitle={target}
      verb={t(spec.verb)}
    >
      {call.isError ? <ErrorNote text={call.modelOutput} />
        : edits.length > 0 ? <PassageEdits edits={edits} />
          : body.trim() ? <MarkdownResult text={body} />
            : description.trim() ? <p className="vk-tool-note">{description}</p>
              : action === "ingest" && call.modelOutput?.trim() ? <MarkdownResult text={call.modelOutput} /> : null}
    </ToolFrame>
  );
}

/** Shows each replaced passage as removed and added rows.
 * 将每个被替换的段落展示为删除行与新增行。 */
function PassageEdits(props: { edits: readonly KnowledgeEdit[] }) {
  const groups = props.edits.map((edit) => [
    ...edit.oldText.split("\n").map((text) => ({ kind: "delete" as const, text })),
    ...edit.newText.split("\n").map((text) => ({ kind: "insert" as const, text })),
  ]);
  return <PlainDiffView groups={groups} />;
}
