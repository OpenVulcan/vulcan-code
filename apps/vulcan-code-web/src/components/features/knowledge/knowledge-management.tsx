import * as React from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { useI18n } from "@/i18n";
import { listKnowledgeCustomWords, listKnowledgeProjectBindings } from "@/service/knowledge-base-service";
import type { WorkbenchSnapshot } from "@/types";
import type { KnowledgeProjectBindingRequest } from "@/types/knowledge-base";
import { KnowledgeReadFailure } from "./knowledge-feedback";
import { useKnowledgeRead } from "./use-knowledge-read";

// Exact library owner and parent mutation ceiling for human-only settings.
// 人工专属设置的精确库所有者及父级变更上限。
interface KnowledgeManagementOwner {
  // Exact current physical library.
  // 精确当前物理知识库。
  libraryId: string;
  // Blocks overlapping lifecycle writes.
  // 阻止重叠生命周期写入。
  busy: boolean;
}

// Project grants are edited only for projects in the authoritative workbench snapshot.
// 仅编辑权威工作台快照中项目的授权。
interface KnowledgeBindingsProps extends KnowledgeManagementOwner {
  // Actual workbench project rows, passed from their owning SkillsArea.
  // 来自所属 SkillsArea 的真实工作台项目行。
  projects: WorkbenchSnapshot["projects"];
  // Persists exact authorization and inherited-default flags.
  // 持久化精确授权及继承默认标记。
  onSave: (request: KnowledgeProjectBindingRequest) => Promise<void>;
}

// Renders human authorization without inventing a library/project fallback.
// 渲染人工授权，不虚构库或项目兜底。
// Parameters: props supplies current projects, exact library and serialized save callback. Returns authorization/default controls.
// 参数：props 提供当前项目、精确库及串行保存回调。返回授权及默认控件。
export function KnowledgeBindings({ libraryId, projects, busy, onSave }: KnowledgeBindingsProps) {
  // All visible grant text uses shared catalogs.
  // 全部可见授权文案使用共享目录。
  const { t } = useI18n();
  // A committed-but-unread grant is distinct from an empty grant set.
  // 已提交但未读回授权与空授权集合不同。
  const [error, setError] = React.useState<string | null>(null);
  // The sole grant source is this library's exact binding list.
  // 唯一授权来源为此库精确绑定列表。
  const load = React.useCallback(() => listKnowledgeProjectBindings(libraryId), [libraryId]);
  // Visit ownership revokes previous-library responses.
  // 访问归属撤销先前知识库响应。
  const read = useKnowledgeRead(libraryId + ":bindings", load);
  // Saves an exact request, then refreshes only this read; returns after the attempt without replaying on failure.
  // 保存精确请求，再仅刷新此读取；尝试后返回，不在失败时重放。
  async function save(request: KnowledgeProjectBindingRequest): Promise<void> {
    setError(null);
    try { await onSave(request); if (!await read.refresh(true)) setError(t("knowledge.changedRefreshFailed")); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }
  return <section className="space-y-3">
    <p className="text-sm text-muted-foreground">{t("knowledge.bindings.description")}</p>
    {read.loading && read.value === null ? <StreamPlaceholder label={t("knowledge.librariesLoading")} /> : null}
    {read.error !== null ? <KnowledgeReadFailure error={read.error} onRetry={read.refresh} /> : null}
    {error !== null ? <pre role="alert" className="select-text whitespace-pre-wrap break-words text-sm text-destructive">{error}</pre> : null}
    {read.value !== null ? <ul className="space-y-2">{projects.map((project) => {
      // Presence means authorization because set_project_binding deletes revoked rows.
      // 存在即表示授权，因为 set_project_binding 删除已撤销行。
      const binding = read.value === null ? undefined : read.value.find((row) => row.projectId === project.id);
      return <li key={project.id} className="flex flex-wrap items-center gap-4 rounded-md border p-3"><strong>{project.name}</strong>
        <label className="flex items-center gap-2"><input type="checkbox" checked={binding !== undefined} disabled={busy || read.loading} onChange={(event) => void save({ libraryId, projectId: project.id, authorized: event.target.checked, defaultEnabled: event.target.checked && binding !== undefined && binding.defaultEnabled })} />{t("knowledge.bindings.authorized")}</label>
        <label className="flex items-center gap-2"><input type="checkbox" checked={binding !== undefined && binding.defaultEnabled} disabled={busy || read.loading || binding === undefined} onChange={(event) => void save({ libraryId, projectId: project.id, authorized: true, defaultEnabled: event.target.checked })} />{t("knowledge.bindings.default")}</label>
      </li>;
    })}</ul> : null}
  </section>;
}

// Custom dictionary mutation callbacks never accept document content.
// 自定义词典变更回调绝不接受文档内容。
interface KnowledgeDictionaryProps extends KnowledgeManagementOwner {
  // Atomically inserts or updates one exact positive-weight term.
  // 原子插入或更新一个精确正权重词语。
  onSave: (word: string, weight: number) => Promise<void>;
  // Removes one exact dictionary key.
  // 移除一个精确词典键。
  onRemove: (word: string) => Promise<void>;
}

// Renders human Jieba terms separately from AI-owned knowledge content.
// 将人工结巴词语与 AI 拥有的知识内容分别渲染。
// Parameters: props supplies exact owner and atomic term callbacks. Returns term editing and current dictionary rows.
// 参数：props 提供精确所有者及原子词语回调。返回词语编辑及当前词典行。
export function KnowledgeDictionary({ libraryId, busy, onSave, onRemove }: KnowledgeDictionaryProps) {
  // Term labels share the active application language.
  // 词语标签共享当前应用语言。
  const { t } = useI18n();
  // The human-selected term and weight stay local until an explicit save.
  // 人工选择的词语及权重在明确保存前保持本地。
  const [word, setWord] = React.useState("");
  // A positive integer is the service-declared dictionary weight shape.
  // 正整数为服务声明的词典权重形状。
  const [weight, setWeight] = React.useState("1");
  // Failures never manufacture an empty successful dictionary.
  // 失败绝不制造空成功词典。
  const [error, setError] = React.useState<string | null>(null);
  // This loader queries only the exact library's configuration.
  // 此加载器仅查询精确知识库配置。
  const load = React.useCallback(() => listKnowledgeCustomWords(libraryId), [libraryId]);
  // Failed reads revoke writable dictionary rows until explicit recovery succeeds.
  // 读取失败撤销可写词典行，直到明确恢复成功。
  const read = useKnowledgeRead(libraryId + ":dictionary", load);
  // Validate the declared u32 shape without truncating fractional text.
  // 验证声明 u32 形状，不截断小数文本。
  const number = Number(weight);
  // The server remains the semantic term/weight authority.
  // 服务仍为词语及权重语义权威。
  const valid = word.trim() !== "" && Number.isInteger(number) && number > 0 && number < 2 ** 32;
  // Executes one exact parent callback and read refresh; returns without automatic mutation retry.
  // 执行一个精确父级回调及读取刷新；返回且不自动重试变更。
  async function perform(action: () => Promise<void>): Promise<void> {
    setError(null);
    try { await action(); if (!await read.refresh(true)) setError(t("knowledge.changedRefreshFailed")); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }
  return <section className="space-y-3">
    <p className="text-sm text-muted-foreground">{t("knowledge.dictionary.description")}</p>
    {read.loading && read.value === null ? <StreamPlaceholder label={t("knowledge.librariesLoading")} /> : null}
    {read.error !== null ? <KnowledgeReadFailure error={read.error} onRetry={read.refresh} /> : null}
    {error !== null ? <pre role="alert" className="select-text whitespace-pre-wrap break-words text-sm text-destructive">{error}</pre> : null}
    <form className="flex flex-wrap items-end gap-2" onSubmit={(event) => { event.preventDefault(); if (valid && !busy && read.value !== null && !read.loading) void perform(() => onSave(word.trim(), number)); }}>
      <label className="grid gap-1">{t("knowledge.dictionary.word")}<Input aria-label={t("knowledge.dictionary.word")} value={word} disabled={busy || read.loading || read.value === null} onChange={(event) => setWord(event.target.value)} /></label>
      <label className="grid gap-1">{t("knowledge.dictionary.weight")}<Input type="number" min={1} step={1} aria-label={t("knowledge.dictionary.weight")} value={weight} disabled={busy || read.loading || read.value === null} onChange={(event) => setWeight(event.target.value)} /></label>
      <Button type="submit" disabled={busy || read.loading || read.value === null || !valid}>{t("memory.common.save")}</Button>
    </form>
    {read.value !== null ? <ul className="space-y-1">{read.value.map((entry) => <li className="flex items-center justify-between gap-2 rounded-md border p-2" key={entry.word}><button type="button" disabled={busy || read.loading} className="text-left" onClick={() => { setWord(entry.word); setWeight(String(entry.weight)); }}>{entry.word} · {entry.weight}</button><Button variant="outline" size="sm" disabled={busy || read.loading} onClick={() => void perform(() => onRemove(entry.word))}>{t("knowledge.dictionary.remove")}</Button></li>)}</ul> : null}
  </section>;
}
