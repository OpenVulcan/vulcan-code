// File cards. A read shows the exact lines returned; an inspection shows entries or facts; a change shows what
// moved in each file as a diffstat in the head and a unified diff in the body.
// 文件卡。读取展示返回的精确行；检查展示条目或事实；变更在标题中以差异统计、在正文中以统一差异展示每个文件的改动。
import * as React from "react";
import { File, FileDiff, FileImage, FileMinus, FilePlus, FileSearch, FileText, Folder, FolderTree, type LucideIcon } from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import type { PersistentChangeSetFile } from "@/service/conversation-event-window-store";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { numberOf, objectOf, textOf } from "../tool-call";
import { fileDisplayPath, fileLineCounts } from "../views/change-set";
import { CodeView, type CodeLine } from "../views/code-view";
import { DiffStat, DiffView } from "../views/diff-view";
import { ErrorNote } from "../views/result-views";

// mediaReadFileName reads the resource name from the host-marked media envelope read_media returns.
// mediaReadFileName 从 read_media 返回的宿主标记媒体信封读取资源名。
export function mediaReadFileName(output: string | null): string | null {
  if (!output) return null;
  let envelope: unknown;
  try {
    envelope = JSON.parse(output);
  } catch {
    return null;
  }
  const record = objectOf(envelope);
  if (record?.$vulcan_native_media_input !== 1) return null;
  return textOf(objectOf(record.resource)?.file_name);
}

/** Renders one read_media call: the media goes to the model, so the card only names it.
 * 渲染一次 read_media 调用：媒体交给模型，因此卡片只标明它。 */
export function MediaReadCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const path = textOf(call.input.path) ?? "";
  const name = mediaReadFileName(call.modelOutput) ?? path.replace(/\\/g, "/").split("/").at(-1) ?? path;
  return (
    <ToolFrame
      {...props}
      icon={FileImage}
      state={call.state}
      target={name}
      targetTitle={path}
      verb={t("toolCard.media.read")}
    >
      {call.isError ? <ErrorNote text={call.modelOutput} /> : null}
    </ToolFrame>
  );
}

/** Renders one file_read call or file read action.
 * 渲染一次 file_read 调用或 file 的读取动作。 */
export function FileReadCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const read = call.host?.kind === "file_access" ? call.host.payload : null;
  const path = textOf(read?.path) ?? textOf(call.input.path) ?? "";
  const lines = React.useMemo(() => readLines(call.modelOutput, numberOf(objectOf(read?.render)?.output_start_line)), [call.modelOutput, read]);
  const range = objectOf(read?.range);
  const start = numberOf(range?.source_start_line) ?? numberOf(call.input.start_line);
  const count = numberOf(range?.line_count);
  const end = start !== null && count !== null ? start + count - 1 : numberOf(call.input.end_line);
  const total = numberOf(read?.total_line_count);
  const empty = read?.read_status === "empty_file" || total === 0;
  const rangeLabel = start !== null && end !== null && !(start === 1 && end === total) ? `L${start}–${end}` : "";
  return (
    <ToolFrame
      {...props}
      icon={FileText}
      meta={empty
        ? <span>{t("toolCard.file.empty")}</span>
        : lines.length > 0 ? <span>{t("toolCard.lines").replace("{count}", String(lines.length))}{total !== null && total > lines.length ? ` / ${total}` : ""}</span> : null}
      state={call.state}
      target={<>{path}{rangeLabel ? <small>{rangeLabel}</small> : null}</>}
      targetTitle={path}
      verb={t("toolCard.file.read")}
    >
      {call.isError
        ? <ErrorNote text={call.modelOutput} />
        : lines.length > 0 ? <CodeView lines={lines} /> : null}
    </ToolFrame>
  );
}

// INSPECT_VERBS names each inspection action of the file tool.
// INSPECT_VERBS 命名 file 工具的每个检查动作。
const INSPECT_VERBS: Record<string, { verb: TranslationKey; icon: LucideIcon }> = {
  list: { icon: FolderTree, verb: "toolCard.file.list" },
  stat: { icon: FileSearch, verb: "toolCard.file.stat" },
};

/** Renders a file list or stat call; move and delete arrive with a change-set and use FileChangeCard.
 * 渲染 file 的 list 或 stat 调用；move 与 delete 携带变更集，由 FileChangeCard 渲染。 */
export function FileInspectCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const action = textOf(call.input.action) ?? "stat";
  const path = textOf(call.input.path) ?? "";
  const operation = INSPECT_VERBS[action] ?? INSPECT_VERBS.stat;
  const entries = action === "list" && call.modelOutput ? listEntries(call.modelOutput) : [];
  const facts = action === "stat" && call.modelOutput ? statFacts(call.modelOutput) : [];
  const truncated = action === "list" && call.modelOutput?.includes("\nTruncated.") === true;
  return (
    <ToolFrame
      {...props}
      icon={operation.icon}
      meta={action === "list" && call.modelOutput && !call.isError
        ? <span>{t("toolCard.file.entries").replace("{count}", String(entries.length))}{truncated ? ` · ${t("toolCard.file.truncated")}` : ""}</span>
        : null}
      state={call.state}
      target={path}
      targetTitle={path}
      verb={t(operation.verb)}
    >
      {call.isError ? <ErrorNote text={call.modelOutput} /> : entries.length > 0 ? (
        <div className="vk-tree">
          {entries.map((entry) => (
            <div data-dir={entry.directory ? "" : undefined} key={entry.path}>
              {entry.directory ? <Folder aria-hidden="true" /> : <File aria-hidden="true" />}
              <span>{entry.path}</span>
              {entry.size !== null ? <small>{formatBytes(entry.size)}</small> : null}
            </div>
          ))}
        </div>
      ) : facts.length > 0 ? (
        <dl className="vk-facts">
          {facts.map((fact) => [<dt key={`k${fact.key}`}>{t(fact.label)}</dt>, <dd key={`v${fact.key}`}>{fact.value}</dd>])}
        </dl>
      ) : null}
    </ToolFrame>
  );
}

// CHANGE_VERBS names the action applied to one file, and its tile icon.
// CHANGE_VERBS 命名应用于单个文件的动作及其图块图标。
const CHANGE_VERBS: Record<PersistentChangeSetFile["change"], { verb: TranslationKey; icon: LucideIcon }> = {
  create: { icon: FilePlus, verb: "toolCard.file.create" },
  delete: { icon: FileMinus, verb: "toolCard.file.delete" },
  modify: { icon: FileDiff, verb: "toolCard.file.modify" },
  rename: { icon: FileDiff, verb: "toolCard.file.rename" },
};

// ATTEMPT_VERBS names a mutation that produced no change-set, by the tool that attempted it.
// ATTEMPT_VERBS 按尝试的工具命名未产生变更集的修改。
const ATTEMPT_VERBS: Record<string, TranslationKey> = {
  apply_patch: "toolCard.file.patch",
  file_edit: "toolCard.file.modify",
  file_write: "toolCard.file.write",
};

/** Renders any call that changed files, or tried to.
 * 渲染任何修改了文件（或尝试修改）的调用。 */
export function FileChangeCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const files = call.event.changeset?.files ?? [];
  const totals = files.reduce((sum, file) => {
    const counts = fileLineCounts(file);
    return counts ? { added: sum.added + counts.added, removed: sum.removed + counts.removed, known: true } : sum;
  }, { added: 0, removed: 0, known: false });
  const single = files.length === 1 ? files[0] : null;
  const verb = single
    ? t(CHANGE_VERBS[single.change].verb)
    : files.length > 1
      ? t("toolCard.file.changeMany").replace("{count}", String(files.length))
      : t(ATTEMPT_VERBS[call.name] ?? "toolCard.file.modify");
  const target = single
    ? single.change === "rename" && single.old_path ? `${single.old_path} → ${fileDisplayPath(single)}` : fileDisplayPath(single)
    : files.length > 1 ? files.map(fileDisplayPath).join(", ") : textOf(call.input.path) ?? "";
  return (
    <ToolFrame
      {...props}
      icon={single ? CHANGE_VERBS[single.change].icon : FileDiff}
      meta={totals.known ? <DiffStat added={totals.added} removed={totals.removed} /> : null}
      state={call.state}
      target={target}
      targetTitle={target}
      verb={verb}
    >
      {files.length === 0
        ? call.isError || call.state === "failed" ? <ErrorNote text={call.modelOutput} /> : null
        : files.map((file, index) => (
            <section key={`${fileDisplayPath(file)}:${index}`}>
              {files.length > 1 ? (
                <div className="vk-diff-file">
                  <span className="vk-file-kind" data-k={file.change[0].toUpperCase()}>{file.change[0].toUpperCase()}</span>
                  <code className="vk-mono vk-truncate">{fileDisplayPath(file)}</code>
                  <span className="vk-tool-meta">{fileLineCounts(file) ? <DiffStat {...fileLineCounts(file)!} /> : null}</span>
                </div>
              ) : null}
              <DiffView file={file} />
            </section>
          ))}
    </ToolFrame>
  );
}

// readLines extracts the numbered "L<n>: text" lines a file read returns inside its fence.
// readLines 提取文件读取在围栏内返回的 "L<n>: text" 编号行。
export function readLines(output: string | null, outputStartLine: number | null): CodeLine[] {
  if (!output) return [];
  const all = output.replace(/\r\n?/g, "\n").split("\n");
  // Without the host offset the body starts after the opening fence.
  // 没有宿主偏移时，正文从开围栏之后开始。
  const start = outputStartLine !== null ? outputStartLine - 1 : all.findIndex((line) => line.startsWith("```")) + 1;
  const lines: CodeLine[] = [];
  for (const line of all.slice(Math.max(0, start))) {
    if (line.startsWith("```")) break;
    const match = /^L(\d+): ?(.*)$/.exec(line);
    if (match) lines.push({ number: Number(match[1]), text: match[2] });
  }
  return lines;
}

// ListEntry is one directory entry from a file list result.
// ListEntry 是文件列表结果中的一个目录条目。
interface ListEntry {
  // Entry path.
  // 条目路径。
  path: string;
  // Whether the entry is a directory.
  // 条目是否为目录。
  directory: boolean;
  // File size in bytes, for files.
  // 文件的字节大小（仅文件）。
  size: number | null;
}

// listEntries parses the "- `path` · kind · N bytes" lines of the stable list output.
// listEntries 解析稳定列表输出中的 "- `path` · kind · N bytes" 行。
export function listEntries(output: string): ListEntry[] {
  const entries: ListEntry[] = [];
  for (const line of output.split(/\r?\n/)) {
    const match = /^- `(.+)` · (\w+)(?: · (\d+) bytes)?$/.exec(line);
    if (match) entries.push({ directory: match[2] === "directory", path: match[1], size: match[3] ? Number(match[3]) : null });
  }
  return entries;
}

// STAT_FACTS maps the stable stat output labels onto catalog keys.
// STAT_FACTS 将稳定的 stat 输出标签映射到目录键。
const STAT_FACTS: Array<[string, TranslationKey]> = [
  ["Type", "toolCard.file.fact.kind"],
  ["Size", "toolCard.file.fact.size"],
  ["Lines", "toolCard.file.fact.lines"],
  ["Encoding", "toolCard.file.fact.encoding"],
  ["Line endings", "toolCard.file.fact.lineEnding"],
  ["BOM", "toolCard.file.fact.bom"],
  ["Final newline", "toolCard.file.fact.finalNewline"],
  ["Modified (Unix ms)", "toolCard.file.fact.modified"],
];

// statFacts reads "Label: value" lines of the stable stat output in a fixed order.
// statFacts 按固定顺序读取稳定 stat 输出中的 "Label: value" 行。
export function statFacts(output: string): Array<{ key: string; label: TranslationKey; value: string }> {
  const facts: Array<{ key: string; label: TranslationKey; value: string }> = [];
  for (const [key, label] of STAT_FACTS) {
    const match = new RegExp(`^${key.replace(/[()]/g, "\\$&")}: (.+)$`, "m").exec(output);
    if (!match) continue;
    let value = match[1].replace(/^`(.*)`$/, "$1");
    if (key === "Size") value = formatBytes(Number(value.replace(/ bytes$/, "")));
    if (key === "Modified (Unix ms)") value = new Date(Number(value)).toLocaleString();
    facts.push({ key, label, value });
  }
  return facts;
}

// formatBytes renders a byte count with a binary unit.
// formatBytes 以二进制单位渲染字节数。
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes)) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
